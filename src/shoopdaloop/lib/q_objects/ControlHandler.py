
from PySide6.QtCore import QObject, Signal, Property, Slot, Qt, Q_ARG, Q_RETURN_ARG, QMetaType
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue
import inspect
import lupa

from ..logging import Logger
from ..lua_qobject_interface import lua_passthrough, qt_typename, lua_int, lua_bool, lua_str, lua_float, lua_callable
from ..backend_wrappers import LoopMode

def as_loop_selector(lua_val):
    def iscoords(l):
        return len(l) == 2 and isinstance(l[0], int) and isinstance(l[1], int)
    def isempty(l):
        return len(l) == 0
    def tolist(l):
        return l if isinstance(l, list) else list(dict(l).values())
    
    maybe_exception = RuntimeError("Could not recognize loop selector type.")
    try:
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
    
    raise ValueError('Failed to interpret loop selector. Loop selector may be a callable (f => bool), [], [x,y], or [[x1,y1],[x2,y2],...]. Selector: {} (type {}). Exception: {}'.format(str(lua_val), lupa.lua_type(lua_val), str(maybe_exception)))

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
        if lupa.lua_type(lua_val) == 'table' or isinstance(lua_val, list):
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

class ControlHandler(QQuickItem):
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
        self.qml_instance_changed.connect(self.introspect)
        self.introspect()
    
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
        [ 'loop_count', lua_loop_selector ],
        [ 'loop_get_mode', lua_loop_selector ],
        [ 'loop_get_length', lua_loop_selector ],
        [ 'loop_get_which_selected' ],
        [ 'loop_get_which_targeted' ],
        [ 'loop_get_by_mode', lua_int ],
        [ 'loop_get_by_track', lua_int ],
        [ 'loop_get_all' ],
        [ 'loop_get_volume', lua_loop_selector ],
        [ 'loop_get_balance', lua_loop_selector ],
        [ 'loop_transition', lua_loop_selector, lua_int, lua_int ],
        [ 'loop_record_n', lua_loop_selector, lua_int, lua_int ],
        [ 'loop_record_with_targeted', lua_loop_selector ],
        [ 'loop_set_balance', lua_loop_selector ],
        [ 'loop_select', lua_loop_selector, lua_bool ],
        [ 'loop_target', lua_loop_selector ],
        [ 'loop_clear', lua_loop_selector ],
        [ 'loop_untarget_all' ],
        [ 'track_get_volume', lua_track_selector ],
        [ 'track_get_muted', lua_track_selector ],
        [ 'track_get_input_muted', lua_track_selector ],
        [ 'track_set_muted', lua_track_selector, lua_bool ],
        [ 'track_set_input_muted', lua_track_selector, lua_bool ],
        [ 'track_set_volume', lua_track_selector, lua_float ],
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
    
    lua_constants = generate_loop_mode_constants()
    
    @Slot()
    def introspect(self):
        if not self._qml_instance:
            return
        for i in range(self._qml_instance.metaObject().methodCount()):
            method = self._qml_instance.metaObject().method(i)
            def call_qml(*args, m=method):
                return_typename = qt_typename(m.returnType())
                name = str(m.name(), 'ascii')
                if return_typename == 'Void':
                    self.logger.debug("Calling void QML method {}".format(name))
                    self._qml_instance.metaObject().invokeMethod(
                        self._qml_instance,
                        name,
                        Qt.ConnectionType.AutoConnection,
                        *[Q_ARG('QVariant', arg) for arg in args]
                    )
                    return
                rval = self._qml_instance.metaObject().invokeMethod(
                    self._qml_instance,
                    name,
                    Qt.ConnectionType.AutoConnection,
                    Q_RETURN_ARG(return_typename),
                    *[Q_ARG('QVariant', arg) for arg in args]
                )
                if isinstance(rval, QJSValue):
                    rval = rval.toVariant()
                self.logger.debug("Result of calling {} QML method {}: {}".format(return_typename, name, str(rval)))
                return rval
            self._methods[str(method.name(), 'ascii')] = {
                'call_qml': call_qml
            }

    @staticmethod
    def allow_qml_override(func):
        def wrapper(self, *args, **kwargs):
            self.logger.debug("Call ControlHandler {} with args {}".format(func.__name__, str(args)))
            if self.qml_instance:
                try:
                    return self._methods[func.__name__ + "_override"]['call_qml'](*args)
                except RuntimeError as e:
                    self.logger.error("Failed to call QML override: {}".format(str(e)))

            self.cache_call([func.__name__, *args])
            raise NotImplementedError(
                "ControlHandler interface {0} not or incorrectly overridden.".format(func.__name__)
            )
        wrapper.__name__ = func.__name__
        wrapper.__doc__ = func.__doc__
        return wrapper

    qml_instance_changed = Signal('QVariant')
    @Property('QVariant', notify=qml_instance_changed)
    def qml_instance(self):
        return self._qml_instance
    @qml_instance.setter
    def qml_instance(self, val):
        if val != self._qml_instance:
            self._qml_instance = val
            self.qml_instance_changed.emit(val)

    # LOOPS
    # loop_selector selects the loop(s) to target.
    # For some interfaces only one loop may be selected (e.g. getters).
    # For some interfaces, the selector may yield multiple loops (e.g. transitions).
    # The selector can be one of:
    # - [track_idx, loop_idx]   (select single by coords)
    # - [[track_idx, loop_idx], [track_idx, loop_idx], ...]  (select multiple by coords)
    # - (loop) => <true/false> (functor which is passed the loop widget and should return true/false)

    @Slot('QVariant', result=int)
    @allow_qml_override
    def loop_count(self, loop_selector):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_count(loop_selector) -> int
        Count the amount of loops given by the selector.
        @shoop_lua_fn_docstring.end
        """
        pass

    @Slot('QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_mode(self, loop_selector):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_mode(loop_selector) -> LoopMode
        Get the current mode of the specified loops.
        @shoop_lua_fn_docstring.end
        """
        pass

    @Slot('QVariant', result='QVariant')
    @allow_qml_override
    def loop_get_length(self, loop_selector):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.loop_get_length(loop_selector) -> int
        Get the length of the specified loops in samples.
        @shoop_lua_fn_docstring.end
        """
        pass
    
    @Slot(result='QVariant')
    @allow_qml_override
    def loop_get_which_selected(self):
        """
        Get the coordinates of all currently selected loops.
        """
        pass

    @Slot(result='QVariant')
    @allow_qml_override
    def loop_get_all(self):
        """
        Get the coordinates of all loops.
        """
        pass
    
    @Slot(result='QVariant')
    @allow_qml_override
    def loop_get_which_targeted(self):
        """
        Get the coordinates of the currently targeted loop, or None if none are targeted.
        """
        pass

    @Slot(int, result='QVariant')
    @allow_qml_override
    def loop_get_by_mode(self, mode):
        """
        Get the coordinates of all loops with the given mode.
        """
        pass

    @Slot(int, result='QVariant')
    @allow_qml_override
    def loop_get_by_track(self, track):
        """
        Get the coordinates of all loops with the given mode.
        """
        pass

    @Slot('QVariant', result=float)
    @allow_qml_override
    def loop_get_volume(self, loop_selector):
        """
        Get the output audio volume of the specified loops.
        """
        pass

    @Slot('QVariant', result=float)
    @allow_qml_override
    def loop_get_balance(self, loop_selector):
        """
        Get the output audio balance of the specified stereo loops.
        """
        pass

    # Loop action interfaces
    @Slot('QVariant', int, int)
    @allow_qml_override
    def loop_transition(self, loop_selector, mode, cycles_delay):
        """
        Transition the given loops. Whether the transition is immediate or synchronized
        depends on the global application "synchronization active" state.
        
        Args:
            loop_selector: The loop(s) to transition.
            mode: The mode to transition to.
            cycles_delay: The amount of cycles to wait before transitioning.
        """
        pass

    @Slot('QVariant', int, int)
    @allow_qml_override
    def loop_record_n(self, loop_selector, n, cycles_delay):
        """
        Record the given loops for N cycles synchronously.

        Args:
            loop_selector: The loop(s) to record.
            n: The amount of cycles to record.
            cycles_delay: The amount of cycles to wait before recording.
        """
        pass

    @Slot('QVariant')
    @allow_qml_override
    def loop_record_with_targeted(self, loop_selector):
        """
        Record the given loops in sync with the currently targeted loop.
        """
        pass

    @Slot('QVariant', int, float)
    @allow_qml_override
    def loop_set_balance(self, loop_selector, balance):
        """
        Set the audio output balance for the specified loops.
        """
        pass

    @Slot('QVariant', bool)
    @allow_qml_override
    def loop_select(self, loop_selector, deselect_others):
        """
        Select the specified loops.

        Args:
            loop_selector: The loop(s) to select.
            deselect_others: Whether to deselect all other loops.
        """
        pass

    @Slot('QVariant')
    @allow_qml_override
    def loop_target(self, loop_selector):
        """
        Target the specified loop. If the selector specifies more than one loop,
        a single loop in the set is chosen arbitrarily.
        """
        pass

    @Slot()
    @allow_qml_override
    def loop_untarget_all(self):
        """
        Untarget the currently targeted loop, if any.
        """
        pass

    @Slot('QVariant')
    @allow_qml_override
    def loop_clear(self, loop_selector):
        """
        Clear the given loops.
        """
        pass

    # track
    # track_selector selects the track(s) to target.
    # For some interfaces only one track may be selected (e.g. getters).
    # For some interfaces, the selector may yield multiple tracks (e.g. set volume).
    # The selector can be one of:
    # - [track_idx, fn] (fn is (track) => true/false, applied to tracks of track)
    # - fn              (fn is (track) => true/false, applied to tracks of all tracks)

    # track getter interfaces
    @Slot('QVariant', result=float)
    @allow_qml_override
    def track_get_volume(self, track_selector):
        """
        Get the volume of the given track.
        """
        pass

    @Slot('QVariant', result=bool)
    @allow_qml_override
    def track_get_muted(self, track_selector):
        """
        Get whether the given track is muted.
        """
        pass

    @Slot('QVariant', result=bool)
    @allow_qml_override
    def track_get_input_muted(self, track_selector):
        """
        Get whether the given track's input is muted.
        """
        pass

    @Slot('QVariant', bool)
    @allow_qml_override
    def track_set_input_muted(self, track_selector, muted):
        """
        Set whether the given track's input is muted.
        """
        pass

    @Slot('QVariant', float)
    @allow_qml_override
    def track_set_volume(self, track_selector, vol):
        """
        Set the given track's volume.
        """
        pass
