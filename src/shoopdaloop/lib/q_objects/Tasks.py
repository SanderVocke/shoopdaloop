from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from .ShoopPyObject import *

from ..logging import Logger

# Keep track of a set of tasks.
# Can itself also be used as a Task.
class Tasks(ShoopQObject):
    def __init__(self, parent=None):
        super(Tasks, self).__init__(parent)
        self._tasks = []
        self._n_done = 0
        self.logger = Logger('Frontend.Tasks')

    def calc_all_done(self):
        return self._n_done >= len(self._tasks)

    # anything_to_do
    anythingToDoChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=anythingToDoChanged)
    def anything_to_do(self):
        return not self.calc_all_done()
    
    @ShoopSlot('QVariant')
    def add_task(self, task):
        self.logger.debug(lambda: 'Adding task {}, anything to do: {}'.format(task, not self.calc_all_done()))
        prev_all_done = self.calc_all_done()
        self._tasks.append(task)
        if not task.anything_to_do:
            self._n_done = self._n_done + 1
        cur_all_done = self.calc_all_done()
        if prev_all_done != cur_all_done:
            self.anythingToDoChanged.emit(not cur_all_done)
        
        task.anythingToDoChanged.connect(self.task_changed)
    
    @ShoopSlot(bool)
    def task_changed(self, anything_to_do):
        self.logger.debug(lambda: 'Task changed, anything to do: {}'.format(not self.calc_all_done()))
        prev_all_done = self.calc_all_done()
        if not anything_to_do:
            self._n_done = self._n_done + 1
        else:
            self._n_done = self._n_done - 1
        cur_all_done = self.calc_all_done()
        if prev_all_done != cur_all_done:
            self.anythingToDoChanged.emit(not cur_all_done)
    
    @ShoopSlot('QVariant')
    def when_finished(self, fn):
        def exec_fn():
            self.logger.debug(lambda: 'All tasks finished, calling callback')
            if callable(fn):
                fn()
            elif isinstance(fn, QJSValue):
                fn.call()
        
        if not self.anything_to_do:
            exec_fn()
        else:
            self.anythingToDoChanged.connect(lambda: exec_fn())
    
