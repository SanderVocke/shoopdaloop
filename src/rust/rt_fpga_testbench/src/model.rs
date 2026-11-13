//! Elaborated port metadata and small generated adapters for Marlin's dynamic API.
use crate::{Result, bits::Bits};
use marlin::verilator::{
    PortDirection, VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions,
    dynamic::{AsDynamicVerilatedModel, DynamicVerilatedModel, VerilatorValue},
    tracing::{TraceFile, Waveform},
};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use std::{
    collections::BTreeMap,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug)]
pub struct Probe {
    pub path: String,
    pub inject: bool,
}
impl Probe {
    pub fn observe(path: &str) -> Self {
        Self {
            path: path.into(),
            inject: false,
        }
    }
    pub fn inject(path: &str) -> Self {
        Self {
            path: path.into(),
            inject: true,
        }
    }
}
pub struct Design {
    pub top: String,
    pub sources: Vec<PathBuf>,
    pub includes: Vec<PathBuf>,
    pub parameters: BTreeMap<String, String>,
    pub probes: Vec<Probe>,
    pub memory: Option<NativeMemory>,
    host_instance: Option<String>,
}
#[derive(Clone, Debug, Hash)]
pub struct NativeMemory {
    pub words: usize,
    pub base: u32,
    pub embedded: bool,
}
impl Design {
    pub fn new(top: &str, sources: Vec<PathBuf>) -> Self {
        Self {
            top: top.into(),
            sources,
            includes: vec![crate::root().join("fpga/state_processor/rtl")],
            parameters: BTreeMap::new(),
            probes: Vec::new(),
            memory: None,
            host_instance: None,
        }
    }
    pub fn parameters(mut self, values: &[(&str, u64)]) -> Self {
        self.parameters
            .extend(values.iter().map(|&(k, v)| (k.into(), v.to_string())));
        self
    }
    pub fn probes(mut self, probes: Vec<Probe>) -> Self {
        self.probes = probes;
        self
    }
    pub fn memory(mut self, words: usize, base: u32) -> Self {
        assert!(words > 0);
        self.memory = Some(NativeMemory {
            words,
            base,
            embedded: true,
        });
        self.sources
            .push(crate::root().join("fpga/axi_memory/tests/axi_direct_memory.sv"));
        self
    }
    pub fn administration(mut self, words: usize) -> Self {
        self.memory = Some(NativeMemory {
            words,
            base: 0,
            embedded: false,
        });
        self
    }
    pub fn administration_at(mut self, words: usize, instance: &str) -> Self {
        self.memory = Some(NativeMemory {
            words,
            base: 0,
            embedded: false,
        });
        self.host_instance = Some(instance.into());
        self
    }
    pub fn build(self, cache: &Path) -> Result<Compiled> {
        crate::report::record_model(&self)?;
        let executable = std::env::var_os("SHOOP_VERILATOR").unwrap_or_else(|| "verilator".into());
        let mut hash = std::hash::DefaultHasher::new();
        self.top.hash(&mut hash);
        self.parameters.hash(&mut hash);
        for value in self.parameters.values() {
            if let Some(file) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                if Path::new(file).is_file() {
                    fs::read(file)?.hash(&mut hash);
                }
            }
        }
        self.memory.hash(&mut hash);
        if let Some(instance) = &self.host_instance {
            instance.hash(&mut hash);
        }
        for p in &self.probes {
            p.path.hash(&mut hash);
            p.inject.hash(&mut hash);
        }
        for source in &self.sources {
            source.hash(&mut hash);
            fs::read(source)?.hash(&mut hash);
        }
        for include in &self.includes {
            for path in fs::read_dir(include)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?
            {
                if path.extension().is_some_and(|s| s == "svh") {
                    fs::read(path)?.hash(&mut hash);
                }
            }
        }
        Command::new(&executable)
            .arg("--version")
            .output()?
            .stdout
            .hash(&mut hash);
        let directory = cache.join(format!("{}-{:016x}", self.top, hash.finish()));
        fs::create_dir_all(&directory)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("elaborate.lock"))?;
        lock.lock()?;
        let xml = directory.join("design.xml");
        if !xml.is_file() {
            let mut command = Command::new(&executable);
            command
                .args(["--xml-only", "--xml-output"])
                .arg(&xml)
                .args(["--top-module", &self.top, "--timescale", "1ns/1ps"]);
            for (k, v) in &self.parameters {
                command.arg(format!("-G{k}={v}"));
            }
            for include in &self.includes {
                command.arg(format!("-I{}", include.display()));
            }
            let output = command.args(&self.sources).output()?;
            fs::write(
                directory.join("elaborate.log"),
                [&output.stdout[..], &output.stderr[..]].concat(),
            )?;
            if !output.status.success() {
                return Err(format!(
                    "{} elaboration failed: {}",
                    self.top,
                    directory.join("elaborate.log").display()
                )
                .into());
            }
        }
        let document = Node::parse(&fs::read_to_string(xml)?)?;
        let netlist = document.child("netlist").ok_or("XML has no netlist")?;
        let top = netlist
            .children
            .iter()
            .find(|n| n.tag == "module" && n.attr("topModule") == Some("1"))
            .ok_or("XML has no top module")?;
        let types = netlist.child("typetable").ok_or("XML has no type table")?;
        let mut ports = Vec::new();
        let mut declarations = Vec::new();
        let mut connections = Vec::new();
        let mut bus_width = 0;
        for var in top
            .children
            .iter()
            .filter(|v| v.tag == "var" && v.attr("dir").is_some())
        {
            let name = var.attr("name").ok_or("port has no name")?.to_owned();
            let width = type_width(types, var.attr("dtype_id").ok_or("port has no type")?)?;
            let input = match var.attr("dir") {
                Some("input") => true,
                Some("output") => false,
                other => return Err(format!("unsupported port direction: {other:?}").into()),
            };
            if name == "m_axi_wdata" {
                bus_width = width;
            }
            let exposed_input = input
                && !(self.memory.as_ref().is_some_and(|m| m.embedded)
                    && name.starts_with("m_axi_"));
            declarations.push(declaration(&name, width, exposed_input));
            connections.push(format!(".{name}({name})"));
            ports.push(Port {
                name,
                width,
                input: exposed_input,
            });
        }
        let mut body = String::new();
        if let Some(memory) = &self.memory {
            assert!(
                matches!(bus_width, 32 | 64),
                "native memory requires a 32/64-bit AXI master"
            );
            for (name, width, input) in [
                ("sim_control", 5, true),
                ("sim_fail_token", 32, true),
                ("sim_fail_seen", 32, false),
                ("sim_busy", 1, false),
                ("sim_active", 1, false),
                ("sim_error", 1, false),
                ("sim_write", 1, false),
                ("sim_index", 32, false),
                ("sim_reads", 64, false),
                ("sim_writes", 64, false),
                ("sim_capacity", 32, false),
                ("sim_host_op", 2, true),
                ("sim_host_address", 32, true),
                ("sim_host_write_data", 32, true),
                ("sim_host_read_data", 32, false),
            ] {
                if ports.iter().any(|p| p.name == name) {
                    continue;
                }
                declarations.push(declaration(name, width, input));
                ports.push(Port {
                    name: name.into(),
                    width,
                    input,
                });
            }
            if memory.embedded {
                body.push_str(&format!("    wire unused_image_done;\n    axi_direct_memory #(.DATA_WIDTH({bus_width}), .WORDS({}), .BASE(32'd{})) native_memory (\n        .sim_image_request(2'b0), .sim_image_done(unused_image_done), .*\n    );\n",memory.words,memory.base));
            }
            let instance = self.host_instance.as_deref().unwrap_or(if memory.embedded {
                "native_memory"
            } else {
                "dut"
            });
            body.push_str(&format!("    assign sim_host_read_data = sim_host_address < {} ? {instance}.samples[sim_host_address] : 32'b0;\n    always @(posedge clk) begin\n        if (sim_host_op != 0) begin\n            if (sim_busy && !reset) $fatal(1, \"host memory access during a transfer\");\n            if (sim_host_op == 1) begin\n                if (sim_host_address >= {}) $fatal(1, \"host memory address outside arena\");\n                {instance}.samples[sim_host_address] = sim_host_write_data;\n            end else if (sim_host_op == 2) begin\n                for (integer index = 0; index < {}; index = index+1) {instance}.samples[index] = sim_host_write_data;\n            end else $fatal(1, \"invalid host memory operation\");\n        end\n    end\n",memory.words,memory.words,memory.words));
        }
        let mut aliases = BTreeMap::new();
        for probe in &self.probes {
            let mut module = top;
            let parts: Vec<_> = probe.path.split('.').collect();
            for part in &parts[..parts.len() - 1] {
                let instance = module
                    .children
                    .iter()
                    .find(|n| {
                        matches!(n.tag.as_str(), "instance" | "begin")
                            && n.attr("name") == Some(part)
                    })
                    .ok_or_else(|| format!("missing instance in probe {}: {part}", probe.path))?;
                if instance.tag == "begin" {
                    module = instance;
                    continue;
                }
                let name = instance
                    .attr("defName")
                    .ok_or("instance has no definition")?;
                module = netlist
                    .children
                    .iter()
                    .find(|n| n.tag == "module" && n.attr("name") == Some(name))
                    .ok_or("instance definition not found")?;
            }
            let field = parts.last().unwrap();
            let (field, index) = if let Some((name, index)) = field.split_once('[') {
                (name, Some(index.trim_end_matches(']').parse::<usize>()?))
            } else {
                (*field, None)
            };
            let var = module
                .children
                .iter()
                .find(|n| n.tag == "var" && n.attr("name") == Some(field))
                .ok_or_else(|| format!("missing probe signal {}", probe.path))?;
            let mut dtype = var.attr("dtype_id").ok_or("probe has no type")?;
            if let Some(index) = index {
                let ty = types
                    .children
                    .iter()
                    .find(|n| n.attr("id") == Some(dtype))
                    .ok_or("missing probe array type")?;
                if ty.tag != "unpackarraydtype" {
                    return Err(
                        format!("indexed probe is not an unpacked array: {}", probe.path).into(),
                    );
                }
                let range = ty.child("range").ok_or("array has no bounds")?;
                let bounds: Vec<_> = range
                    .children
                    .iter()
                    .map(|v| {
                        let value = v
                            .attr("name")
                            .ok_or("array bound has no constant")?
                            .split_once('\'')
                            .ok_or("unsupported array bound")?
                            .1
                            .trim_start_matches('s');
                        let value = value
                            .strip_prefix('h')
                            .ok_or("unsupported array bound radix")?;
                        Ok(usize::from_str_radix(value, 16)?)
                    })
                    .collect::<Result<_>>()?;
                if bounds.len() != 2
                    || index < *bounds.iter().min().unwrap()
                    || index > *bounds.iter().max().unwrap()
                {
                    return Err(format!("probe index outside array: {}", probe.path).into());
                }
                dtype = ty.attr("sub_dtype_id").ok_or("array has no element type")?;
            }
            let width = type_width(types, dtype)?;
            let name = format!(
                "probe_{}",
                probe
                    .path
                    .replace('.', "_p_")
                    .replace('[', "_i_")
                    .replace(']', "")
            );
            declarations.push(declaration(&name, width, false));
            body.push_str(&format!("    assign {name} = dut.{};\n", probe.path));
            ports.push(Port {
                name: name.clone(),
                width,
                input: false,
            });
            aliases.insert(probe.path.clone(), name.clone());
            if probe.inject {
                let valid = format!("{name}_force");
                let value = format!("{name}_value");
                declarations.push(declaration(&valid, 1, true));
                declarations.push(declaration(&value, width, true));
                ports.push(Port {
                    name: valid.clone(),
                    width: 1,
                    input: true,
                });
                ports.push(Port {
                    name: value.clone(),
                    width,
                    input: true,
                });
                body.push_str(&format!("    always @({valid} or {value}) begin\n        if ({valid}) force dut.{} = {value};\n        else release dut.{};\n    end\n", probe.path, probe.path));
            }
        }
        let parameters = if self.parameters.is_empty() {
            String::new()
        } else {
            format!(
                " #( {} )",
                self.parameters
                    .iter()
                    .map(|(k, v)| format!(".{k}({v})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let wrapper = directory.join("bench_top.sv");
        let text = format!(
            "module bench_top (\n{}\n);\n    {}{} dut ({});\n{}endmodule\n",
            declarations.join(",\n"),
            self.top,
            parameters,
            connections.join(", "),
            body
        );
        if fs::read_to_string(&wrapper).ok().as_deref() != Some(&text) {
            fs::write(&wrapper, text)?;
        }
        let mut sources = self.sources;
        sources.push(wrapper.clone());
        let options = VerilatorRuntimeOptions::default().verilator_executable(
            crate::root()
                .join("src/rust/rt_fpga_sim/verilator-wrapper")
                .into(),
        );
        let runtime = VerilatorRuntime::new2(
            directory.join("model"),
            &sources,
            &self.includes,
            [],
            options,
        )?;
        Ok(Compiled {
            runtime,
            ports,
            aliases,
            wrapper,
            _lock: lock,
        })
    }
}
fn declaration(name: &str, width: usize, input: bool) -> String {
    format!(
        "    {} wire [{}:0] {name}",
        if input { "input" } else { "output" },
        width - 1
    )
}
#[derive(Clone)]
pub struct Port {
    pub name: String,
    pub width: usize,
    pub input: bool,
}
pub struct Compiled {
    runtime: VerilatorRuntime,
    ports: Vec<Port>,
    aliases: BTreeMap<String, String>,
    wrapper: PathBuf,
    _lock: std::fs::File,
}
impl Compiled {
    pub fn session(&self, waveform: Option<&Path>) -> Result<Model<'_>> {
        let default_waveform = crate::report::waveform(
            self.wrapper
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        );
        let waveform = waveform.or(default_waveform.as_deref());
        if let Some(path) = waveform {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        let signatures: Vec<_> = self
            .ports
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.width - 1,
                    0,
                    if p.input {
                        PortDirection::Input
                    } else {
                        PortDirection::Output
                    },
                )
            })
            .collect();
        // Keep trace support in cached models; a file opens only for an explicit path.
        let config = VerilatedModelConfig::default().enable_tracing(Some(Waveform::Vcd));
        let mut model = self.runtime.create_dyn_model(
            "bench_top",
            self.wrapper.to_str().ok_or("non-UTF8 source path")?,
            &signatures,
            config,
        )?;
        let trace = waveform.map(|p| model.open_trace(p)).transpose()?;
        let mut session = Model {
            trace,
            model,
            ports: self
                .ports
                .iter()
                .map(|p| (p.name.clone(), p.clone()))
                .collect(),
            aliases: self.aliases.clone(),
            inputs: BTreeMap::new(),
        };
        for p in &self.ports {
            if p.input {
                session.set_bits(&p.name, &Bits::zero(p.width));
            }
        }
        session.eval();
        Ok(session)
    }
}
pub struct Model<'a> {
    trace: Option<TraceFile<'a>>,
    model: DynamicVerilatedModel<'a>,
    ports: BTreeMap<String, Port>,
    aliases: BTreeMap<String, String>,
    inputs: BTreeMap<String, Bits>,
}
impl Model<'_> {
    fn name<'b>(&'b self, name: &'b str) -> &'b str {
        self.aliases.get(name).map(String::as_str).unwrap_or(name)
    }
    pub fn has(&self, name: &str) -> bool {
        self.ports.contains_key(self.name(name))
    }
    pub fn width(&self, name: &str) -> usize {
        self.ports[self.name(name)].width
    }
    pub fn set(&mut self, name: &str, value: u64) {
        let name = self.aliases.get(name).map(String::as_str).unwrap_or(name);
        let port = &self.ports[name];
        assert!(port.input, "cannot drive output {name}");
        if port.width > 64 {
            let name = name.to_owned();
            self.set_bits(&name, &Bits::from_u64(value));
            return;
        }
        assert!(
            port.width == 64 || value >> port.width == 0,
            "value {value} does not fit input {name}"
        );
        self.model
            .pin_value(name, VerilatorValue::QData(value))
            .unwrap();
        if let Some(bits) = self.inputs.get_mut(name) {
            bits.0[0] = value as u32;
            if bits.0.len() > 1 {
                bits.0[1] = (value >> 32) as u32;
            }
        } else {
            self.inputs
                .insert(name.into(), Bits::from_u64(value).resized(port.width));
        }
    }
    pub fn set_signed(&mut self, name: &str, value: i32) {
        self.set(name, u64::from(value as u32));
    }
    pub fn set_bits(&mut self, name: &str, bits: &Bits) {
        let name = self.name(name).to_owned();
        let port = &self.ports[&name];
        assert!(port.input, "cannot drive output {name}");
        let bits = bits.resized(port.width);
        let value = match port.width {
            0..=8 => VerilatorValue::CData(bits.get(0, 8) as u8),
            9..=16 => VerilatorValue::SData(bits.get(0, 16) as u16),
            17..=32 => VerilatorValue::IData(bits.get(0, 32) as u32),
            33..=64 => VerilatorValue::QData(bits.get(0, 64)),
            _ => VerilatorValue::WDataInP(&bits.0),
        };
        self.model.pin_value(&name, value).unwrap();
        self.inputs.insert(name, bits);
    }
    pub fn bits(&self, name: &str) -> Bits {
        let name = self.name(name);
        if self.ports[name].input {
            return self.inputs[name].clone();
        }
        match self.model.read_value(name).unwrap() {
            VerilatorValue::CData(v) => Bits::from_u64(u64::from(v)),
            VerilatorValue::SData(v) => Bits::from_u64(u64::from(v)),
            VerilatorValue::IData(v) => Bits::from_u64(u64::from(v)),
            VerilatorValue::QData(v) => Bits::from_u64(v),
            VerilatorValue::WDataOutP(v) => Bits(v.into_vec()),
            _ => unreachable!(),
        }
    }
    pub fn get(&self, name: &str) -> u64 {
        let name = self.name(name);
        let port = &self.ports[name];
        assert!(port.width <= 64);
        if port.input {
            let v = &self.inputs[name].0;
            u64::from(v[0]) | (u64::from(v.get(1).copied().unwrap_or(0)) << 32)
        } else {
            self.model.read_scalar(name).unwrap()
        }
    }
    pub fn set_all(&mut self, values: &[(&str, u64)]) {
        for &(name, value) in values {
            self.set(name, value);
        }
    }
    pub fn inject(&mut self, path: &str, value: Option<&Bits>) {
        let name = self.name(path).to_owned();
        self.set(&format!("{name}_force"), u64::from(value.is_some()));
        if let Some(value) = value {
            self.set_bits(&format!("{name}_value"), value);
        }
        self.eval();
    }
    pub fn eval(&mut self) {
        self.model.eval();
    }
    pub fn dump(&mut self, ps: u64) {
        if let Some(trace) = &mut self.trace {
            trace.dump(ps);
        }
    }
}

#[derive(Default)]
struct Node {
    tag: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
}
impl Node {
    fn parse(text: &str) -> Result<Self> {
        let mut reader = Reader::from_str(text);
        let mut stack = vec![Node::default()];
        loop {
            match reader.read_event()? {
                Event::Start(e) => stack.push(Self::start(&reader, &e)?),
                Event::Empty(e) => {
                    let node = Self::start(&reader, &e)?;
                    stack.last_mut().unwrap().children.push(node);
                }
                Event::End(_) => {
                    let node = stack.pop().ok_or("unbalanced XML")?;
                    stack
                        .last_mut()
                        .ok_or("unbalanced XML")?
                        .children
                        .push(node);
                }
                Event::Eof => break,
                _ => (),
            }
        }
        stack
            .pop()
            .and_then(|mut n| n.children.pop())
            .ok_or_else(|| "empty XML".into())
    }
    fn start(reader: &Reader<&[u8]>, e: &BytesStart<'_>) -> Result<Self> {
        let mut attrs = BTreeMap::new();
        for a in e.attributes() {
            let a = a?;
            attrs.insert(
                std::str::from_utf8(a.key.as_ref())?.into(),
                a.decoded_and_normalized_value(quick_xml::XmlVersion::default(), reader.decoder())?
                    .into_owned(),
            );
        }
        Ok(Self {
            tag: std::str::from_utf8(e.name().as_ref())?.into(),
            attrs,
            children: Vec::new(),
        })
    }
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
    fn child(&self, tag: &str) -> Option<&Self> {
        self.children.iter().find(|n| n.tag == tag)
    }
}
fn type_width(types: &Node, id: &str) -> Result<usize> {
    let ty = types
        .children
        .iter()
        .find(|n| n.attr("id") == Some(id))
        .ok_or_else(|| format!("missing dtype {id}"))?;
    if ty.tag == "basicdtype" {
        let left = ty.attr("left").unwrap_or("0").parse::<i64>()?;
        let right = ty.attr("right").unwrap_or("0").parse::<i64>()?;
        return Ok((left - right).unsigned_abs() as usize + 1);
    }
    if matches!(ty.tag.as_str(), "refdtype" | "enumdtype") {
        return type_width(
            types,
            ty.attr("sub_dtype_id").ok_or("type alias has no target")?,
        );
    }
    Err(format!("unsupported port dtype {}: {id}", ty.tag).into())
}
