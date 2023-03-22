import sys
import os

from .lib.q_objects.Application import Application

def main():
    app = Application('ShoopDaLoop',
        '{}/../lib/qml/applications/shoopdaloop_main.qml'.format(script_dir))
    return app.exec()