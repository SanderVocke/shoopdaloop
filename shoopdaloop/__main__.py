import sys
import os
import traceback

from .lib.q_objects.Application import Application
from .lib.logging import *

script_pwd = os.path.dirname(__file__)

def main():
    logger = Logger("Frontend.Main")
    try:
        app = Application('ShoopDaLoop',
            '{}/lib/qml/applications/shoopdaloop_main.qml'.format(script_pwd))
        app.exec()
    except Exception as e:
        logger.error("Exception: " + str(e) + "\n" + traceback.format_exc())
        exit()