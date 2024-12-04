from PySide6.QtQml import QJSValue

from .backend_wrappers import PyAudioDriverType, PortConnectability, DontWaitForSync, DontAlignToSyncImmediately
from .types import KeyEventType
import shoop_py_backend

# Create a QJSValue which contains a multitude of ShoopDaLoop constant
# values to be used within the javascript context.
def create_js_constants(engine):
    rval = engine.newObject()

    def add_enum(name, enum):
        enum_object = engine.newObject()
        for item in list(enum):
            if item.name == 'names':
                continue
            enum_object.setProperty(item.name, item.value)
        rval.setProperty(name, enum_object)
    
    def add_rust_enum(name, enum):
        enum_object = engine.newObject()
        for _name, val in enum.enum_items().items():
            enum_object.setProperty(_name, val)
        rval.setProperty(name, enum_object)
    
    add_rust_enum('LoopMode', shoop_py_backend.LoopMode)
    add_rust_enum('ChannelMode', shoop_py_backend.ChannelMode)
    add_enum('AudioDriverType', PyAudioDriverType)
    add_rust_enum('PortDirection', shoop_py_backend.PortDirection)
    add_rust_enum('FXChainType', shoop_py_backend.FXChainType)
    add_enum('KeyEventType', KeyEventType)
    add_rust_enum('PortDataType', shoop_py_backend.PortDataType)
    add_enum('PortConnectability', PortConnectability)

    # Other constants
    rval.setProperty('DontWaitForSync', DontWaitForSync)
    rval.setProperty('DontAlignToSyncImmediately', DontAlignToSyncImmediately)

    return rval

