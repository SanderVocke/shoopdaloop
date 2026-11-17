// Copyright (C) 2024 Ethan Uppal.
//
// This Source Code Form is subject to the terms of the Mozilla Public License,
// v. 2.0. If a copy of the MPL was not distributed with this file, You can
// obtain one at https://mozilla.org/MPL/2.0/.

//! Support for dynamic models.

use crate::WideOut;
use libloading::Library;
pub use marlin_verilator_stable::dynamic::{
    AsDynamicVerilatedModel, DynamicVerilatedModelError, VerilatorValue,
};
use marlin_verilator_stable::{core::PortDirection, types};
use std::{collections::HashMap, ffi, slice};

impl<const WORDS: usize> From<WideOut<WORDS>> for VerilatorValue<'_> {
    fn from(value: WideOut<WORDS>) -> Self {
        Self::WDataOutP(value.inner.into())
    }
}
#[derive(Clone, Copy)]
enum Read {
    C(extern "C" fn(*mut ffi::c_void) -> types::CData),
    S(extern "C" fn(*mut ffi::c_void) -> types::SData),
    I(extern "C" fn(*mut ffi::c_void) -> types::IData),
    Q(extern "C" fn(*mut ffi::c_void) -> types::QData),
    W(extern "C" fn(*mut ffi::c_void) -> types::WDataOutP),
}
#[derive(Clone, Copy)]
enum Pin {
    C(extern "C" fn(*mut ffi::c_void, types::CData)),
    S(extern "C" fn(*mut ffi::c_void, types::SData)),
    I(extern "C" fn(*mut ffi::c_void, types::IData)),
    Q(extern "C" fn(*mut ffi::c_void, types::QData)),
    W(extern "C" fn(*mut ffi::c_void, types::WDataInP)),
}
#[derive(Clone, Copy)]
pub(crate) struct DynamicPortInfo {
    pub(crate) width: usize,
    pub(crate) direction: PortDirection,
    read: Option<Read>,
    pin: Option<Pin>,
}
impl DynamicPortInfo {
    pub(crate) fn bind(
        library: &Library,
        top: &str,
        name: &str,
        width: usize,
        direction: PortDirection,
    ) -> Result<Self, libloading::Error> {
        let getter = format!("ffi_V{top}_read_{name}");
        let setter = format!("ffi_V{top}_pin_{name}");
        // SAFETY: generated FFI signatures are selected by the elaborated port width.
        let (read, pin) = unsafe {
            let read = if matches!(direction, PortDirection::Output | PortDirection::Inout) {
                Some(match width {
                    0..=8 => Read::C(*library.get(getter.as_bytes())?),
                    9..=16 => Read::S(*library.get(getter.as_bytes())?),
                    17..=32 => Read::I(*library.get(getter.as_bytes())?),
                    33..=64 => Read::Q(*library.get(getter.as_bytes())?),
                    _ => Read::W(*library.get(getter.as_bytes())?),
                })
            } else {
                None
            };
            let pin = if matches!(direction, PortDirection::Input | PortDirection::Inout) {
                Some(match width {
                    0..=8 => Pin::C(*library.get(setter.as_bytes())?),
                    9..=16 => Pin::S(*library.get(setter.as_bytes())?),
                    17..=32 => Pin::I(*library.get(setter.as_bytes())?),
                    33..=64 => Pin::Q(*library.get(setter.as_bytes())?),
                    _ => Pin::W(*library.get(setter.as_bytes())?),
                })
            } else {
                None
            };
            (read, pin)
        };
        Ok(Self {
            width,
            direction,
            read,
            pin,
        })
    }
}
/// A model constructed at runtime, borrowing its compiled library and context.
pub struct DynamicVerilatedModel<'ctx> {
    pub(crate) ports: HashMap<String, DynamicPortInfo>,
    pub(crate) name: String,
    pub(crate) main: *mut ffi::c_void,
    pub(crate) eval_main: extern "C" fn(*mut ffi::c_void),
    pub(crate) library: &'ctx Library,
    pub(crate) opened_trace: bool,
}
impl<'ctx> AsDynamicVerilatedModel<'ctx> for DynamicVerilatedModel<'ctx> {
    fn eval(&mut self) {
        (self.eval_main)(self.main);
    }
    fn read(
        &self,
        port: impl Into<String>,
    ) -> Result<VerilatorValue<'_>, DynamicVerilatedModelError> {
        self.read_value(&port.into())
    }
    fn pin(
        &mut self,
        port: impl Into<String>,
        value: impl Into<VerilatorValue<'ctx>>,
    ) -> Result<(), DynamicVerilatedModelError> {
        self.pin_value(&port.into(), value.into())
    }
}
impl<'ctx> DynamicVerilatedModel<'ctx> {
    fn info(&self, port: &str) -> Result<&DynamicPortInfo, DynamicVerilatedModelError> {
        self.ports
            .get(port)
            .ok_or_else(|| DynamicVerilatedModelError::NoSuchPort {
                top_module: self.name.clone(),
                port: port.into(),
                source: None,
            })
    }
    fn direction_error(
        &self,
        port: &str,
        info: &DynamicPortInfo,
        attempted_direction: PortDirection,
    ) -> DynamicVerilatedModelError {
        DynamicVerilatedModelError::InvalidPortDirection {
            top_module: self.name.clone(),
            port: port.into(),
            direction: info.direction,
            attempted_direction,
        }
    }
    fn width_error(
        &self,
        port: &str,
        width: usize,
        lower: usize,
        higher: usize,
    ) -> DynamicVerilatedModelError {
        DynamicVerilatedModelError::InvalidPortWidth {
            top_module: self.name.clone(),
            port: port.into(),
            width,
            attempted_lower: lower,
            attempted_higher: higher,
        }
    }
    /// Reads a scalar without allocating a port name or packed word buffer.
    pub fn read_scalar(&self, port: &str) -> Result<u64, DynamicVerilatedModelError> {
        let info = self.info(port)?;
        match info
            .read
            .ok_or_else(|| self.direction_error(port, info, PortDirection::Output))?
        {
            Read::C(f) => Ok(u64::from(f(self.main))),
            Read::S(f) => Ok(u64::from(f(self.main))),
            Read::I(f) => Ok(u64::from(f(self.main))),
            Read::Q(f) => Ok(f(self.main)),
            Read::W(_) => Err(self.width_error(port, info.width, 0, 64)),
        }
    }
    pub fn read_value(&self, port: &str) -> Result<VerilatorValue<'_>, DynamicVerilatedModelError> {
        let info = self.info(port)?;
        match info
            .read
            .ok_or_else(|| self.direction_error(port, info, PortDirection::Output))?
        {
            Read::C(f) => Ok(f(self.main).into()),
            Read::S(f) => Ok(f(self.main).into()),
            Read::I(f) => Ok(f(self.main).into()),
            Read::Q(f) => Ok(f(self.main).into()),
            Read::W(f) => {
                let pointer = f(self.main);
                let length = info.width.div_ceil(32);
                assert!(!pointer.is_null() && pointer.is_aligned());
                assert!(length
                    .checked_mul(size_of::<types::EData>())
                    .is_some_and(|n| n < isize::MAX as usize));
                // SAFETY: FFI returns this model's packed array; copy it before any evaluation can mutate it.
                Ok(VerilatorValue::WDataOutP(
                    unsafe { slice::from_raw_parts(pointer, length) }.into(),
                ))
            }
        }
    }
    /// Copies inputs immediately. Wide slices need not outlive the model.
    pub fn pin_value(
        &mut self,
        port: &str,
        value: VerilatorValue<'_>,
    ) -> Result<(), DynamicVerilatedModelError> {
        let info = self.info(port)?;
        let pin = info
            .pin
            .ok_or_else(|| self.direction_error(port, info, PortDirection::Input))?;
        let supplied_width = value.width();
        if supplied_width < info.width {
            return Err(self.width_error(port, info.width, 0, supplied_width));
        }
        let scalar = match &value {
            VerilatorValue::CData(v) => u64::from(*v),
            VerilatorValue::SData(v) => u64::from(*v),
            VerilatorValue::IData(v) => u64::from(*v),
            VerilatorValue::QData(v) => *v,
            VerilatorValue::WDataInP(v) => {
                u64::from(v.first().copied().unwrap_or(0))
                    | (u64::from(v.get(1).copied().unwrap_or(0)) << 32)
            }
            VerilatorValue::WDataOutP(_) => {
                panic!("cannot drive an output word buffer; provide a borrowed input slice")
            }
        };
        match pin {
            Pin::C(f) => f(self.main, scalar as types::CData),
            Pin::S(f) => f(self.main, scalar as types::SData),
            Pin::I(f) => f(self.main, scalar as types::IData),
            Pin::Q(f) => f(self.main, scalar),
            Pin::W(f) => {
                let VerilatorValue::WDataInP(words) = value else {
                    return Err(self.width_error(port, info.width, 65, supplied_width));
                };
                if words.len() < info.width.div_ceil(32) {
                    return Err(self.width_error(port, info.width, 65, words.len() * 32));
                }
                f(self.main, words.as_ptr());
            }
        }
        Ok(())
    }
    /// Opens the single trace associated with a dynamically bound model.
    pub fn open_trace(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<crate::tracing::TraceFile<'ctx>, Box<dyn std::error::Error>> {
        use crate::ffi_names;
        assert!(!self.opened_trace, "a model supports only one open trace");
        let path = ffi::CString::new(path.as_ref().as_os_str().as_encoded_bytes())?;
        // SAFETY: the library is generated with these exact tracing signatures.
        unsafe {
            let open: libloading::Symbol<
                extern "C" fn(*mut ffi::c_void, *const ffi::c_char) -> *mut ffi::c_void,
            > = self
                .library
                .get(ffi_names::open_trace(&self.name).as_bytes())?;
            let dump = *self.library.get(ffi_names::TRACE_DUMP.as_bytes())?;
            let open_next = *self.library.get(ffi_names::TRACE_OPEN_NEXT.as_bytes())?;
            let flush = *self.library.get(ffi_names::TRACE_FLUSH.as_bytes())?;
            let close = *self
                .library
                .get(ffi_names::TRACE_CLOSE_AND_DELETE.as_bytes())?;
            let handle = open(self.main, path.as_ptr());
            self.opened_trace = true;
            Ok(crate::tracing::__private::new_trace(
                handle, dump, open_next, flush, close,
            ))
        }
    }
}
