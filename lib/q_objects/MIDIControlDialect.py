from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, pyqtProperty, QMetaObject, Qt

class MIDIControlDialect(QObject):
    def __init__(self, parent=None):
        super(MIDIControlDialect, self).__init__(parent)
        self._substitutions = dict()
        self._input_rules = []
        self._loop_state_changed_formulas = dict()

    # substitutions
    # This is a map of string -> string. Any occurrence of the
    # key string in any of the formulas used in this dialect
    # are replaced by their substitution.
    substitutionsChanged = pyqtSignal('QVariantMap')
    @pyqtProperty('QVariantMap', notify=substitutionsChanged)
    def substitutions(self):
        return self._substitutions
    @substitutions.setter
    def substitutions(self, s):
        if self._substitutions != s:
            self._substitutions = s
            self.substitutionsChanged.emit(s)
    
    # input rules
    # This is a list of MIDIControlInputRule objects.
    # The input rules filter incoming MIDI messages and execute
    # commands predicated on a condition.
    inputRulesChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=inputRulesChanged)
    def inputRules(self):
        return self._input_rules
    @inputRules.setter
    def inputRules(self, s):
        if self._input_rules != s:
            self._input_rules = s
            self.inputRulesChanged.emit(s)
    
    # loop mode output formulas
    # This is a map of integer -> string.
    # Each entry represents a formula to be executed when a loop
    # changes its mode to the given integer value.
    loopStateChangeFormulasChanged = pyqtSignal('QMap<int, QVariant>')
    @pyqtProperty('QMap<int, QVariant>', notify=loopStateChangeFormulasChanged)
    def loopStateChangeFormulas(self):
        return self._loop_state_changed_formulas
    @loopStateChangeFormulas.setter
    def loopStateChangeFormulas(self, s):
        if self._loop_state_changed_formulas != s:
            self._loop_state_changed_formulas = s
            self.loopStateChangeFormulasChanged.emit(s)