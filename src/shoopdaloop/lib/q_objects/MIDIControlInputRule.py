from PySide6.QtCore import QObject, Signal, Slot, Property, QMetaObject, Qt

class MIDIControlInputRule(QObject):
    def __init__(self, parent=None):
        super(MIDIControlInputRule, self).__init__(parent)
        self._substitutions = dict()

    # substitutions
    # This is a map of string -> string. Any occurrence of the
    # key string in any of the formulas used in this dialect
    # are replaced by their substitution.
    substitutionsChanged = Signal('QVariantMap')
    @Property('QVariantMap', notify=substitutionsChanged)
    def substitutions(self):
        return self._substitutions
    @substitutions.setter
    def substitutions(self, s):
        if self._substitutions != s:
            self._substitutions = s
            self.substitutionsChanged(s)