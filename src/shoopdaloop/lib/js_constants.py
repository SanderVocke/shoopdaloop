from PySide6.QtQml import QJSValue

from .backend_wrappers import LoopMode, ChannelMode, AudioDriverType, PortDirection, FXChainType, PortDataType, PortConnectability, DontWaitForSync, DontAlignToSyncImmediately
from .types import KeyEventType

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
    
    add_enum('LoopMode', LoopMode)
    add_enum('ChannelMode', ChannelMode)
    add_enum('AudioDriverType', AudioDriverType)
    add_enum('PortDirection', PortDirection)
    add_enum('FXChainType', FXChainType)
    add_enum('KeyEventType', KeyEventType)
    add_enum('PortDataType', PortDataType)
    add_enum('PortConnectability', PortConnectability)

    # Other constants
    rval.setProperty('DontWaitForSync', DontWaitForSync)
    rval.setProperty('DontAlignToSyncImmediately', DontAlignToSyncImmediately)

    return rval

