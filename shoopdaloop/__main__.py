import sys
import os

from .lib.q_objects.Application import Application

from .lib.cpp_objects import *

script_pwd = os.path.dirname(__file__)

def main():
    test = RenderAudioWaveform()
    print(test.__dict__)
    #print(test.give_me_a_string())

    app = Application('ShoopDaLoop',
        '{}/lib/qml/applications/shoopdaloop_main.qml'.format(script_pwd))
    return app.exec()