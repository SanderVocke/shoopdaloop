
from PySide6.QtCore import QObject, Signal, Property, Slot, Qt, Q_ARG, Q_RETURN_ARG, QMetaType ,QTimer
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue
import inspect
import lupa

from .ShoopPyObject import *

from ..logging import Logger
from ..lua_qobject_interface import lua_passthrough, qt_typename, lua_int, lua_bool, lua_str, lua_float, lua_callable
from ..backend_wrappers import LoopMode, DontAlignToSyncImmediately, DontWaitForSync

def as_loop_selector(lua_val):
    def iscoords(l):
        return len(l) == 2 and isinstance(l[0], int) and isinstance(l[1], int)
    def isempty(l):
        return len(l) == 0
    def tolist(l):
        return l if isinstance(l, list) else list(dict(l).values())
    
    maybe_exception = RuntimeError("Could not recognize loop selector type.")
    try:
        if lua_val == None:
            return []
        if lupa.lua_type(lua_val) == 'table' or isinstance(lua_val, list):
            aslist = tolist(lua_val)
            if iscoords(aslist) or isempty(aslist):
                return aslist # is coordinates or empty
            aslists = [tolist(val) for val in aslist]
            assert(list(set([iscoords(l) for l in aslists])) == [True])
            return aslists
        elif lupa.lua_type(lua_val) == 'function' or callable(lua_val):
            # Lua functors may be passed, can be called from Python later.
            return lua_val
    except Exception as e:
        maybe_exception = e
    
    raise ValueError('Failed to interpret loop selector. Loop selector may be a callable (f => bool), nil, [], [x,y], or [[x1,y1],[x2,y2],...]. Selector: {} (type {}). Exception: {}'.format(str(lua_val), lupa.lua_type(lua_val), str(maybe_exception)))

def as_track_selector(lua_val):
    def iscoord(l):
        # track coordinates are based on the track.
        return isinstance(l, int)
    def isempty(l):
        return len(l) == 0
    def tolist(l):
        return l if isinstance(l, list) else list(dict(l).values())
    
    maybe_exception = RuntimeError("Could not recognize track selector type.")
    try:
        if iscoord(lua_val):
            return [lua_val]
        elif lupa.lua_type(lua_val) == 'table' or isinstance(lua_val, list):
            aslist = tolist(lua_val)
            if isempty(aslist):
                return aslist # is empty
            assert(list(set([iscoord(l) for l in aslist])) == [True])
            return aslist
        elif lupa.lua_type(lua_val) == 'function' or callable(lua_val):
            # Lua functors may be passed, can be called from Python later.
            return lua_val
    except Exception as e:
        maybe_exception = e
    
    raise ValueError('Failed to interpret track selector. track selector may be a callable (f => bool), [], [track_idx], or [tidx1,tidx2,...]. Selector: {} (type {}). Exception: {}'.format(str(lua_val), lupa.lua_type(lua_val), str(maybe_exception)))


lua_loop_selector = [ 'QVariant', as_loop_selector ]
lua_track_selector = [ 'QVariant', as_track_selector ]

def allow_qml_override(func):
    def wrapper(self, *args, **kwargs):
        self.logger.debug(lambda: "Call ControlHandler {} with args {}".format(func.__name__, str(args)))
        # Call func() in most cases just for code coverage
        func(self, *args, **kwargs)
        if self.qml_instance:
            try:
                override_name = func.__name__ + "_override"
                self.logger.trace(lambda: f"Forward to {override_name}, args {[*args[0]]}")
                return self._methods[override_name]['call_qml'](*args[0])
            except RuntimeError as e:
                self.logger.error(lambda: "Failed to call QML override on QML instance {}: {}".format(str(e), self.qml_instance))
                return None
        
        self.cache_call([func.__name__, *args[0]])
        raise NotImplementedError(
            "ControlHandler interface {0} not or incorrectly overridden.".format(func.__name__)
        )
    wrapper.__name__ = func.__name__
    wrapper.__doc__ = func.__doc__
    return wrapper
class ControlHandler(ShoopQQuickItem):
    """
    Defines the API through which ShoopDaLoop can be controlled externally
    (used by e.g. MIDI controllers and Lua scripts).
    In principle the methods of this class should be overridden (they will all throw unimplemented errors).
    However, if the exceptions are caught and discarded, this class can also be used for testing, as it will
    cache the calls made.
    """
    
    def __init__(self, parent=None):
        super(ControlHandler, self).__init__(parent)
        self.logger = Logger('Frontend.ControlHandler')
        self.clear_call_cache()
        self._qml_instance = None
        self._methods = dict()
        self._introspected_qml_instance = None
        self.introspect()
    
    def to_py_val(self, val):
        if isinstance(val, QJSValue):
            return val.toVariant()
        return val
    
    def clear_call_cache(self):
        self.logger.debug(lambda: 'Clearing call cache.')
        self._call_cache = []
    
    def cache_call(self, elems):
        self.logger.debug(lambda: 'Logging call: {}'.format(elems))
        self._call_cache.append(elems)
    
    def cached_calls(self):
        return self._call_cache

    # Specify slots which can be made accessible to Lua. Specified as
    # [ method_name, arg1_type, arg2_type, ... ]
    # Where each type is:
    # [ PySide_type, convert_fn ]
    # (convert_fn will be called on the value passed to Python from Lua)
    lua_interfaces = [
        [ 'loop_count', lua_loop_selector ],
        [ 'loop_get_mode', lua_loop_selector ],
        [ 'loop_get_next_mode', lua_loop_selector ],
        [ 'loop_get_next_mode_delay', lua_loop_selector ],
        [ 'loop_get_length', lua_loop_selector ],
        [ 'loop_get_which_selected' ],
        [ 'loop_get_which_targeted' ],
        [ 'loop_get_by_mode', lua_int ],
        [ 'loop_get_by_track', lua_int ],
        [ 'loop_get_all' ],
        [ 'loop_get_gain', lua_loop_selector ],
        [ 'loop_get_gain_fader', lua_loop_selector ],
        [ 'loop_set_gain', lua_loop_selector, lua_float ],
        [ 'loop_set_gain_fader', lua_loop_selector, lua_float ],
        [ 'loop_get_balance', lua_loop_selector ],
        [ 'loop_set_balance', lua_loop_selector, lua_float ],
        [ 'loop_transition', lua_loop_selector, lua_int, lua_int, lua_int ],
        [ 'loop_trigger', lua_loop_selector, lua_int ],
        [ 'loop_record_n', lua_loop_selector, lua_int, lua_int ],
        [ 'loop_record_with_targeted', lua_loop_selector ],
        [ 'loop_select', lua_loop_selector, lua_bool ],
        [ 'loop_target', lua_loop_selector ],
        [ 'loop_toggle_selected', lua_loop_selector ],
        [ 'loop_toggle_targeted', lua_loop_selector ],
        [ 'loop_clear', lua_loop_selector ],
        [ 'loop_clear_all' ],
        [ 'loop_adopt_ringbuffers', lua_loop_selector, lua_int, lua_int, lua_int, lua_int ],
        [ 'loop_trigger_grab', lua_loop_selector ],
        [ 'loop_compose_add_to_end', lua_loop_selector, lua_loop_selector, lua_bool ],
        [ 'track_get_gain', lua_track_selector ],
        [ 'track_set_gain', lua_track_selector, lua_float ],
        [ 'track_get_gain_fader', lua_track_selector ],
        [ 'track_set_gain_fader', lua_track_selector, lua_float ],
        [ 'track_get_input_gain', lua_track_selector ],
        [ 'track_set_input_gain', lua_track_selector, lua_float ],
        [ 'track_get_input_gain_fader', lua_track_selector ],
        [ 'track_set_input_gain_fader', lua_track_selector, lua_float ],
        [ 'track_get_balance', lua_track_selector ],
        [ 'track_set_balance', lua_track_selector, lua_float ],
        [ 'track_get_muted', lua_track_selector ],
        [ 'track_get_input_muted', lua_track_selector ],
        [ 'track_set_muted', lua_track_selector, lua_bool ],
        [ 'track_set_input_muted', lua_track_selector, lua_bool ],
        [ 'set_apply_n_cycles', lua_int ],
        [ 'get_apply_n_cycles' ],
        [ 'set_solo', lua_bool ],
        [ 'get_solo' ],
        [ 'set_sync_active', lua_bool ],
        [ 'get_sync_active' ],
        [ 'set_play_after_record', lua_bool ],
        [ 'get_play_after_record' ],
        [ 'set_default_recording_action', lua_str ],
        [ 'get_default_recording_action' ]
    ]

    def generate_loop_mode_constants():
        # @shoop_lua_enum_docstring.start
        # shoop_control.constants.LoopMode_
        # Constants to represent the various modes a loop can be in or transitioned to.
        # Unknown
        # Stopped
        # Playing
        # Recording
        # PlayingDryThroughWet 
        # RecordingDryIntoWet
        # @shoop_lua_enum_docstring.end
        rval = []
        for i in list(LoopMode):
            rval.append(['LoopMode_' + i.name, i.value])
        return rval

    # @shoop_lua_enum_docstring.start
    # shoop_control.constants.Loop_
    # Special values to pass to loop functions.
    # DontWaitForSync
    # DontAlignToSyncImmediately
    # @shoop_lua_enum_docstring.end
    
    lua_constants = generate_loop_mode_constants() + [
        [ 'Loop_DontWaitForSync', DontWaitForSync ],
        [ 'Loop_DontAlignToSyncImmediately', DontAlignToSyncImmediately ]
    ]
    
    introspectedQmlInstanceChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=introspectedQmlInstanceChanged)
    def introspected_qml_instance(self):
        return self._introspected_qml_instance
    
    @ShoopSlot()
    def introspect(self):
        self.logger.debug(lambda: f"Introspecting QML instance {self.qml_instance} to find overridden interfaces.")
        if not self._qml_instance:
            self.logger.debug(lambda: f"No QML instance found yet.")
            return
        
        outer_self = self

        class QmlCallable:
            def __init__(self, instance, method):
                self.method = method
                self.instance = instance
            
            def __call__(self, *args):
                return_typename = qt_typename(self.method.returnType())
                name = str(self.method.name(), 'ascii')
                if return_typename == 'Void':
                    outer_self.logger.debug(lambda: "Calling void QML method {}".format(name))
                    self.instance.metaObject().invokeMethod(
                        self.instance,
                        name,
                        Qt.ConnectionType.AutoConnection,
                        *[Q_ARG('QVariant', arg) for arg in args]
                    )
                    return
                outer_self.logger.debug(lambda: "Calling {} QML method {} with args {}".format(return_typename, name, args))                
                rval = self.instance.metaObject().invokeMethod(
                    self.instance,
                    name,
                    Qt.ConnectionType.AutoConnection,
                    Q_RETURN_ARG(return_typename),
                    *[Q_ARG('QVariant', arg) for arg in args]
                )
                if isinstance(rval, QJSValue):
                    rval = rval.toVariant()
                outer_self.logger.debug(lambda: "Result of calling {} QML method {}: {}".format(return_typename, name, str(rval)))
                return rval

        self._methods = {}
        for i in range(self._qml_instance.metaObject().methodCount()):
            method = self._qml_instance.metaObject().method(i)
            self._methods[str(method.name(), 'ascii')] = {
                'call_qml': QmlCallable(self._qml_instance, method)
            }
        self._introspected_qml_instance = self._qml_instance
        self.introspectedQmlInstanceChanged.emit(self._qml_instance)

    qmlInstanceChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=qmlInstanceChanged)
    def qml_instance(self):
        self.logger.debug(lambda: f'getter {self._qml_instance}')
        return self._qml_instance
    @qml_instance.setter
    def qml_instance(self, val):
        if val != self._qml_instance:
            self.logger.debug(lambda: f'qml_instance -> {val}')
            self._qml_instance = val
            self.qmlInstanceChanged.emit(val)

    @ShoopSlot(list, 'QVariant', result=int)
    @allow_qml_override
    def loop_count(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_count(loop_selector) -> int
        Count the amount of loops given by the selector.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_mode(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_mode(loop_selector) -> list[LoopMode]
        Get the current mode of the specified loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_next_mode(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_next_mode(loop_selector) -> list[ LoopMode or nil ]
        For the specified loops, get the upcoming mode transition, if any.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_next_mode_delay(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_next_mode_delay(loop_selector) -> list[ int or nil ]
        For the specified loops, get the upcoming mode transition delay in cycles, if any.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_length(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_length(loop_selector) -> list[int]
        Get the length of the specified loops in samples.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_which_selected(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_which_selected() -> [[x1,y1],[x2,y2],...]
        Get the coordinates of all currently selected loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_all(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_all() -> [[x1,y1],[x2,y2],...]
        Get the coordinates of all loops.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_which_targeted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_which_targeted() -> [x,y] | nil
        Get the coordinates of the currently targeted loop, or None if none are targeted.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_by_mode(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_by_mode(mode) -> [[x1,y1],[x2,y2],...]
        Get the coordinates of all loops with the given mode.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_by_track(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_by_track(track) -> [[x1,y1],[x2,y2],...]
        Get the coordinates of all loops with the given mode.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_gain(loop_selector) -> list[float]
        Get the output audio gain of the specified loops as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_gain(loop_selector) -> list[float]
        Get the output audio gain fader position as a fraction of its total range (0-1) of the given loop.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_set_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_set_gain(loop_selector, gain)
        Set the output audio gain of the specified loops as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_set_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_set_gain(loop_selector)
        Set the output audio gain fader position as a fraction of its total range (0-1) of the given loop.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def loop_get_balance(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_balance(loop_selector) -> list[float]
        Get the output audio balance of the specified stereo loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    # Loop action interfaces
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_transition(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_transition(loop_selector, mode, maybe_cycles_delay, maybe_align_to_sync_at)
        Transition the given loops.
        Pass shoop_control.constants.Loop_DontWaitForSync and shoop_control.constants.Loop_DontAlignToSyncImmediately to maybe_cycles_delay and maybe_align_to_sync_at respectively, to disable them.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_trigger(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_trigger(loop_selector, mode)
        Trigger the loop with the given mode. Equivalent to pressing the loop's button in the UI.
        That means the way the trigger is interpreted also depends on the global controls for e.g. sync.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_trigger_grab(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_trigger_grab(loop_selector, mode)
        Trigger a ringbuffer grab on the given loop. Equivalent to pressing the grab button.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_record_n(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_record_n(loop_selector, n_cycles, cycles_delay)
        Record the given loops for N cycles synchronously.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_record_with_targeted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_record_with_targeted(loop_selector)
        Record the given loops in sync with the currently targeted loop.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_set_balance(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_set_balance(loop_selector, balance)
        Set the audio output balance for the specified loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_select(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_select(loop_selector, deselect_others)
        Select the specified loops. If deselect_others is true, all other loops are deselected.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_toggle_selected(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_toggle_selected(loop_selector)
        Toggle selection on the specified loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_target(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_target(loop_selector)
        Target the specified loop. If the selector specifies more than one loop, a single loop in the set is chosen arbitrarily.
        If nil or no loop is passed, the targeted loop is cleared.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_toggle_targeted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_toggle_targeted(loop_selector)
        Target the specified loop or untarget it if already targeted. If the selector specifies more than one loop, a single loop in the set is chosen arbitrarily.
        If nil or no loop is passed, the targeted loop is cleared.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_clear(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_clear(loop_selector)
        Clear the given loops.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_clear_all(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_clear_all()
        Clear all loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_adopt_ringbuffers(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_adopt_ringbuffers(loop_selector, reverse_cycle_start, cycles_length, go_to_cycle, go_to_mode)
        For all channels in the given loops, grab the data currently in the ringbuffer and
        set it as the content (i.e. after-the-fact-recording or "grab").
        reverse_cycle_start sets the start offset for playback. 0 means to play what was being
        recorded in the current sync loop cycle, 1 means start from the previous cycle, etc.
        go_to_cycle and go_to_mode can control the cycle and mode the loop will have right after adopting.
        cycles_length sets the loop length.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def loop_compose_add_to_end(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_compose_add_to_end(loop_selector, loop_selector, parallel)
        Add a loop to the a composition. The first argument is the singular target loop in which
        the composition is stored. If empty, a composition is created.
        The second argument is the loop that should be added.
        The third argument is whether the addition should be parallel to the existing composition.
        If false, the loop is added to the end.
        @shoop_lua_fn_docstring.end
        """
        pass

    # track getter interfaces
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_gain(track_selector) -> list[float]
        Get the gain of the given track(s) as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_balance(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_balance(track_selector) -> list[float]
        Get the balance of the given track(s) as a value between -1 and 1.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_gain_fader(track_selector) -> list[float]
        Get the gain of the given track(s) as a fraction of its total range (0-1).
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_input_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_input_gain(track_selector) -> list[float]
        Get the input gain of the given track(s) as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_input_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_input_gain_fader(track_selector) -> list[float]
        Get the input gain of the given track(s) as a fraction of its total range (0-1).
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_muted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_muted(track_selector) -> list[bool]
        Get whether the given track(s) is/are muted.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_muted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_muted(track_selector, bool)
        Set whether the given track is muted.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=list)
    @allow_qml_override
    def track_get_input_muted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_get_input_muted(track_selector) -> list[bool]
        Get whether the given tracks' input(s) is/are muted.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_input_muted(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_input_muted(track_selector, muted)
        Set whether the given track's input is muted.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_gain(track_selector, vol)
        Set the given track's gain as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_balance(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_balance(track_selector, val)
        Set the given track's balance as a value between -1 and 1.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_gain_fader(track_selector, vol)
        Set the given track's gain as a fraction of its total range (0-1).
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_input_gain(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_input_gain(track_selector, vol)
        Set the given track's input gain as a gain factor.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def track_set_input_gain_fader(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.track_set_input_gain_fader(track_selector, vol)
        Set the given track's input gain as a fraction of its total range (0-1).
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def set_apply_n_cycles(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.set_apply_n_cycles(n)
        Set the amount of sync loop cycles future actions will be executed for.
        Setting to 0 will disable this - actions will be open-ended.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=int)
    @allow_qml_override
    def get_apply_n_cycles(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.get_apply_n_cycles(n)
        Get the amount of sync loop cycles future actions will be executed for.
        0 means disabled.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def set_solo(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.set_solo(val)
        Set the global "solo" control state.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=bool)
    @allow_qml_override
    def get_solo(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.get_solo() -> bool
        Get the global "solo" control state.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def set_sync_active(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.set_sync_active(val)
        Set the global "sync_active" control state.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=bool)
    @allow_qml_override
    def get_sync_active(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.get_sync_active() -> bool
        Get the global "sync_active" control state.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def set_play_after_record(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.set_play_after_record(val)
        Set the global "play_after_record" control state.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=bool)
    @allow_qml_override
    def get_play_after_record(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.get_play_after_record() -> bool
        Get the global "play_after_record" control state.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant')
    @allow_qml_override
    def set_default_recording_action(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.set_default_recording_action(val)
        Set the global "default recording action" control state. Valid values are 'record' or 'grab' - others are ignored.
        @shoop_lua_fn_docstring.end
        """
        pass

    @ShoopSlot(list, 'QVariant', result=str)
    @allow_qml_override
    def get_default_recording_action(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.get_default_recording_action() -> string
        Get the global "default recording action" control state ('record' or 'grab').
        @shoop_lua_fn_docstring.end
        """
        pass
