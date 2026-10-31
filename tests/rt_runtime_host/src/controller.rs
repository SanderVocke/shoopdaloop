use crate::{config, node_id, ports, wire};
use rt_runtime::graph::{
    compile, Catalog, Connection, Endpoint, GraphDescription, NodeSpec, PortType,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Controller {
    catalog: Catalog,
    descriptor: [u8; 128],
    steps: u32,
    edges: u32,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Controller {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self {
            catalog: Catalog::new(
                config(),
                &[NodeSpec {
                    id: node_id(),
                    ports: ports(),
                }],
            )
            .expect("fixture catalog"),
            descriptor: [0; 128],
            steps: 0,
            edges: 0,
        }
    }
    /// Allocating compiler runs only in the control-side instance.
    pub fn compile_graph(&mut self, variant: u32) -> u32 {
        let host = |kind, port| Endpoint {
            node: None,
            kind,
            port,
        };
        let node = |kind, port| Endpoint {
            node: Some(node_id()),
            kind,
            port,
        };
        let graph = if variant == 2 {
            GraphDescription::default()
        } else {
            GraphDescription {
                nodes: vec![node_id()],
                connections: vec![
                    Connection {
                        source: host(PortType::Midi, 0),
                        destination: node(PortType::Midi, 0),
                    },
                    Connection {
                        source: node(PortType::Audio, variant % 2),
                        destination: host(PortType::Audio, 0),
                    },
                    Connection {
                        source: node(PortType::Midi, 0),
                        destination: host(PortType::Midi, 0),
                    },
                ],
            }
        };
        let plan = compile(&self.catalog, &graph).expect("valid control graph");
        let descriptor = plan.descriptor();
        self.steps = descriptor.steps.len() as u32;
        self.edges = descriptor.connections.len() as u32;
        self.descriptor.fill(0);
        let mut offset = 0;
        for step in &descriptor.steps {
            wire::encode_step(*step, &mut self.descriptor[offset..offset + 12]);
            offset += 12;
        }
        for edge in &descriptor.connections {
            wire::encode_edge(*edge, &mut self.descriptor[offset..offset + 24]);
            offset += 24;
        }
        offset as u32
    }
    pub fn descriptor_ptr(&self) -> usize {
        self.descriptor.as_ptr() as usize
    }
    pub fn steps(&self) -> u32 {
        self.steps
    }
    pub fn edges(&self) -> u32 {
        self.edges
    }
    pub fn set_marker(&mut self, marker: u32) {
        crate::set_marker(marker);
    }
    pub fn marker(&self) -> u32 {
        crate::marker()
    }
}
impl Controller {
    pub fn descriptor(&self) -> &[u8] {
        &self.descriptor[..self.steps as usize * 12 + self.edges as usize * 24]
    }
}
impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
