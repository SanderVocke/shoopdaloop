// See shoop_loop.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;
use crate::shoop_py_backend::audio_channel::AudioChannel;
use crate::shoop_py_backend::midi_channel::MidiChannel;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum LoopMode {
    Unknown = backend_bindings::LoopMode::Unknown as isize,
    Stopped = backend_bindings::LoopMode::Stopped as isize,
    Playing = backend_bindings::LoopMode::Playing as isize,
    Recording = backend_bindings::LoopMode::Recording as isize,
    Replacing = backend_bindings::LoopMode::Replacing as isize,
    PlayingDryThroughWet = backend_bindings::LoopMode::PlayingDryThroughWet as isize,
    RecordingDryIntoWet = backend_bindings::LoopMode::RecordingDryIntoWet as isize,
}

impl TryFrom<backend_bindings::LoopMode> for LoopMode {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::LoopMode) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::LoopMode::Unknown => Ok(LoopMode::Unknown),
            backend_bindings::LoopMode::Stopped => Ok(LoopMode::Stopped),
            backend_bindings::LoopMode::Playing => Ok(LoopMode::Playing),
            backend_bindings::LoopMode::Recording => Ok(LoopMode::Recording),
            backend_bindings::LoopMode::Replacing => Ok(LoopMode::Replacing),
            backend_bindings::LoopMode::PlayingDryThroughWet => Ok(LoopMode::PlayingDryThroughWet),
            backend_bindings::LoopMode::RecordingDryIntoWet => Ok(LoopMode::RecordingDryIntoWet),
        }
    }
}

#[pyclass]
pub struct LoopState {
    #[pyo3(get)]
    pub mode : LoopMode,
    #[pyo3(get)]
    pub length : u32,
    #[pyo3(get)]
    pub position : u32,
    #[pyo3(get)]
    pub maybe_next_mode : Option<LoopMode>,
    #[pyo3(get)]
    pub maybe_next_mode_delay : Option<u32>,
}

impl LoopState {
    pub fn new(obj : backend_bindings::LoopState) -> Self {
        return LoopState {
            mode : LoopMode::try_from(obj.mode).unwrap(),
            length : obj.length,
            position : obj.position,
            maybe_next_mode : match obj.maybe_next_mode {
                Some(v) => Some (LoopMode::try_from(v).unwrap()),
                None => None
            },
            maybe_next_mode_delay : obj.maybe_next_mode_delay,
        };
    }
}

#[pyclass]
pub struct Loop {
    pub obj : backend_bindings::Loop,
}

#[pymethods]
impl Loop {
    #[new]
    fn py_new(backend_session : &BackendSession) -> PyResult<Self> {
        Ok(Loop { obj: backend_bindings::Loop::new(&backend_session.obj).unwrap() })
    }

    fn add_audio_channel(&self, mode : i32) -> PyResult<AudioChannel> {
        match backend_bindings::ChannelMode::try_from(mode) {
            Ok(value) => Ok(AudioChannel { obj: self.obj.add_audio_channel(value).unwrap() }),
            Err(_) => {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode"))
            }
        }
    }

    fn add_midi_channel(&self, mode : i32) -> PyResult<MidiChannel> {
        match backend_bindings::ChannelMode::try_from(mode) {
            Ok(value) => Ok(MidiChannel { obj: self.obj.add_midi_channel(value).unwrap() }),
            Err(_) => {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode"))
            }
        }
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    #[pyo3(signature = (to_mode, maybe_cycles_delay=None, maybe_to_sync_at_cycle=None))]
    fn transition(&self, to_mode: i32, maybe_cycles_delay: Option<i32>, maybe_to_sync_at_cycle: Option<i32>) -> PyResult<()> {                                                
        let to_mode = backend_bindings::LoopMode::try_from(to_mode)                                                                                                           
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid loop mode"))?;                                                                              
        self.obj.transition(to_mode, maybe_cycles_delay.unwrap_or(-1), maybe_to_sync_at_cycle.unwrap_or(-1))                                                                  
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Transition failed: {:?}", e)))                                                            
    }                                                                                                                                                                                                                                                                                                                                                
                                                                                                                                                                              
    fn get_state(&self) -> PyResult<LoopState> {                                                                                              
        let state = self.obj.get_state()                                                                                                                                      
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Get state failed: {:?}", e)))?;                                                           
        Ok(LoopState::new(state))
    }                                                                                                                                                                         
                                                                                                                                                                              
    fn set_length(&self, length: u32) -> PyResult<()> {                                                                                                                       
        self.obj.set_length(length)                                                                                                                                           
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Set length failed: {:?}", e)))                                                            
    }                                                                                                                                                                         
                                                                                                                                                                              
    fn set_position(&self, position: u32) -> PyResult<()> {                                                                                                                   
        self.obj.set_position(position)                                                                                                                                       
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Set position failed: {:?}", e)))                                                          
    }                                                                                                                                                                         
                                                                                                                                                                              
    fn clear(&self, length: u32) -> PyResult<()> {                                                                                                                            
        self.obj.clear(length)                                                                                                                                                
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Clear failed: {:?}", e)))                                                                 
    }                                                                                                                                                                         

    #[pyo3(signature = (loop_ref=None))]                                                                                                                                                           
    fn set_sync_source(&self, loop_ref: Option<&Loop>) -> PyResult<()> {                                                                                                      
        self.obj.set_sync_source(loop_ref.map(|l| &l.obj))                                                                                                                    
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Set sync source failed: {:?}", e)))                                                       
    }                                                                                                                                                                         

    #[pyo3(signature = (reverse_start_cycle=None, cycles_length=None, go_to_cycle=None, go_to_mode=0))]                                                                                                                                                   
    fn adopt_ringbuffer_contents(&self, reverse_start_cycle: Option<i32>, cycles_length: Option<i32>, go_to_cycle: Option<i32>, go_to_mode: i32) -> PyResult<()> {            
        let go_to_mode = backend_bindings::LoopMode::try_from(go_to_mode)                                                                                                     
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid loop mode"))?;                                                                              
        self.obj.adopt_ringbuffer_contents(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode)                                                                       
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Adopt ringbuffer contents failed: {:?}", e)))                                             
    }
}
#[pyfunction]
#[pyo3(signature = (loops, to_state=0, maybe_cycles_delay=None, maybe_to_sync_at_cycle=None))]                                                                                                                                                                   
pub fn transition_multiple_loops(loops: Vec<PyRef<Loop>>, to_state: i32, maybe_cycles_delay: Option<i32>, maybe_to_sync_at_cycle: Option<i32>) -> PyResult<()> {                          
    let to_state = backend_bindings::LoopMode::try_from(to_state)                                                                                                         
        .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid loop mode"))?;                                                                              
    let rust_loops: Vec<_> = loops.iter().map(|l| &l.obj).collect();                                                                                                      
    backend_bindings::transition_multiple_loops(&rust_loops, to_state, maybe_cycles_delay.unwrap_or(-1), maybe_to_sync_at_cycle.unwrap_or(-1))                            
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Transition multiple failed: {:?}", e)))                                                   
}  

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<Loop>()?;
    m.add_class::<LoopMode>()?;
    m.add_function(wrap_pyfunction!(transition_multiple_loops, m)?)?;
    Ok(())
}