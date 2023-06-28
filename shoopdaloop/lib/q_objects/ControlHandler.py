
from PySide6.QtCore import QObject, Signal, Property, Slot
import inspect
import lupa

from ..logging import Logger

def as_loop_selector(lua_val):
    def iscoords(l):
        return len(l) == 2 and isinstance(l[0], int) and isinstance(l[1], int)
    
    if lupa.lua_type(lua_val) == 'function':
        # Lua functors may be passed, can be called from Python later.
        return lua_val
    
    try:
        assert lupa.lua_type(lua_val) == 'table'
        aslist = list(dict(lua_val).values())
        if iscoords(aslist):
            return aslist # is coordinates
        aslists = [list(dict(val).values()) for val in aslist]
        assert(list(set([iscoords(l) for l in aslists])) == [True])
        return aslists
    except Exception as e:
        raise ValueError('Failed to intepret loop selector. Loop selector may be a callable (f => bool), [x,y], or [[x1,y1],[x2,y2],...]. Exception: {}'.format(str(e)))

# Defines the API through which ShoopDaLoop can be controlled externally
# (used by e.g. MIDI controllers and Lua scripts).
# In principle the methods of this class should be overridden (they will all throw unimplemented errors).
# However, if the exceptions are caught and discarded, this class can also be used for testing, as it will
# cache the calls made.
class ControlHandler(QObject):
    def __init__(self, parent=None):
        super(ControlHandler, self).__init__(parent)
        self.logger = Logger('Frontend.ControlHandlerInterface')
        self.clear_call_cache()
    
    def to_py_val(self, val):
        if isinstance(val, QJSValue):
            return val.toVariant()
        return val
    
    def clear_call_cache(self):
        self.logger.debug('Clearing call cache.')
        self._call_cache = []
    
    def cache_call(self, elems):
        self.logger.debug('Logging call: {}'.format(elems))
        self._call_cache.append(elems)
    
    def cached_calls(self):
        return self._call_cache

    # For introspection
    lua_interfaces = [
        [ 'loop_is_playing', as_loop_selector ],
        [ 'loop_is_selected', as_loop_selector ],
        [ 'loop_is_targeted', as_loop_selector ],
        [ 'loop_get_volume', as_loop_selector ],
        [ 'loop_get_balance', as_loop_selector ],
        [ 'loop_play', as_loop_selector ],
        [ 'loop_stop', as_loop_selector ],
        [ 'loop_record', as_loop_selector ],
        [ 'loop_record_n', as_loop_selector ],
        [ 'loop_set_balance', as_loop_selector ],
        [ 'loop_play_dry_through_wet', as_loop_selector ],
        [ 'loop_re_record_fx', as_loop_selector ],
        [ 'loop_play_solo', as_loop_selector ],
        [ 'loop_select', as_loop_selector ],
        [ 'loop_target', as_loop_selector ],
        [ 'loop_deselect', as_loop_selector ],
        [ 'loop_untarget', as_loop_selector ],
        [ 'loop_toggle_selected', as_loop_selector ],
        [ 'loop_toggle_targeted', as_loop_selector ],
        [ 'loop_toggle_playing', as_loop_selector ],
        [ 'port_get_volume', as_loop_selector ],
        [ 'port_get_muted', as_loop_selector ],
        # 'port_get_input_muted',
        # 'port_mute',
        # 'port_mute_input',
        # 'port_unmute',
        # 'port_unmute_input',
        # 'port_set_volume',
    ]

    # LOOPS
    # loop_selector selects the loop(s) to target.
    # For some interfaces only one loop may be selected (e.g. getters).
    # For some interfaces, the selector may yield multiple loops (e.g. transitions).
    # The selector can be one of:
    # - [track_idx, loop_idx]   (select single by coords)
    # - [[track_idx, loop_idx], [track_idx, loop_idx], ...]  (select multiple by coords)
    # - (loop) => <true/false> (functor which is passed the loop widget and should return true/false)

    # Loop getter interfaces
    Slot('QVariant')
    def loop_is_playing(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_is_selected(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_is_targeted(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_get_volume(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_get_balance(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    # Loop action interfaces
    Slot('QVariant', int, bool)
    def loop_play(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, bool)
    def loop_stop(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, bool)
    def loop_record(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, int, bool)
    def loop_record_n(self, loop_selector, n, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, n, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, float)
    def loop_set_balance(self, loop_selector, balance):
        self.cache_call([inspect.stack()[0][3], loop_selector, balance])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, bool)
    def loop_play_dry_through_wet(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, bool)
    def loop_re_record_fx(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', int, bool)
    def loop_play_solo(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_select(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_target(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_deselect(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_untarget(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_toggle_selected(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def loop_toggle_targeted(self, loop_selector):
        self.cache_call([inspect.stack()[0][3], loop_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    # Loop internally implemented interfaces
    Slot('QVariant', int, bool)
    def loop_toggle_playing(self, loop_selector, cycles_delay, wait_sync):
        self.cache_call([inspect.stack()[0][3], loop_selector, cycles_delay, wait_sync])
        if loop_is_playing (loop_selector):
            return loop_stop(loop_selector, cycles_delay, wait_sync)
        else:
            return loop_play(loop_selector, cycles_delay, wait_sync)    

    # PORTS
    # port_selector selects the port(s) to target.
    # For some interfaces only one port may be selected (e.g. getters).
    # For some interfaces, the selector may yield multiple ports (e.g. set volume).
    # The selector can be one of:
    # - [track_idx, fn] (fn is (port) => true/false, applied to ports of track)
    # - fn              (fn is (port) => true/false, applied to ports of all tracks)

    # Port getter interfaces
    Slot('QVariant')
    def port_get_volume(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def port_get_muted(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def port_get_input_muted(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    # Port action interfaces
    Slot('QVariant')
    def port_mute(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def port_mute_input(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def port_unmute(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant')
    def port_unmute_input(self, port_selector):
        self.cache_call([inspect.stack()[0][3], port_selector])
        raise NotImplementedError('ControlHandler interface not overridden')

    Slot('QVariant', float)
    def port_set_volume(self, port_selector, vol):
        self.cache_call([inspect.stack()[0][3], port_selector, vol])
        raise NotImplementedError('ControlHandler interface not overridden')
