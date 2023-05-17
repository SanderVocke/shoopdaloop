import sys
import os

from .lib.q_objects.Application import Application

script_pwd = os.path.dirname(__file__)

def main():
    app = Application('ShoopDaLoop',
        '{}/lib/qml/applications/shoopdaloop_main.qml'.format(script_pwd))
    return app.exec()