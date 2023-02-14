#!/bin/python

import sys
import os
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/..'
)

from lib.backend import *
import time
import curses
from sshkeyboard import listen_keyboard

print ("Starting a single loop with stereo audio and MIDI.")
be = Backend('test_single_loop')
loop = be.create_loop()
audio1 = loop.add_audio_channel()
audio2 = loop.add_audio_channel()
midi = loop.add_midi_channel()
audio1_in = be.open_port('audio_in_1', PortType.Audio, PortDirection.Input)
audio1_out = be.open_port('audio_out_1', PortType.Audio, PortDirection.Output)
audio2_in = be.open_port('audio_in_2', PortType.Audio, PortDirection.Input)
autio2_out = be.open_port('audio_out_2', PortType.Audio, PortDirection.Output)
midi_in = be.open_port('midi_in', PortType.Midi, PortDirection.Input)
midi_out = be.open_port('midi_out', PortType.Midi, PortDirection.Output)
audio1.connect(audio1_in)
audio1.connect(audio1_out)
audio2.connect(audio1_in)
audio2.connect(audio1_out)
midi.connect(midi_in)
midi.connect(midi_out)

print("Loop ready. Key commands:")
print("  s : stop")
print("  r : record")
print("  p : play")
print("  c : clear")
print("  m : play muted")

def press(key):
    if key == 'r':
        print ('record')
        loop.transition(LoopState.Recording, 0, False)
    elif key == 'p':
        print ('play')
        loop.transition(LoopState.Playing, 0, False)
    elif key == 's':
        print ('stop')
        loop.transition(LoopState.Stopped, 0, False)
    elif key == 'm':
        print ('play muted')
        loop.transition(LoopState.PlayingMuted, 0, False)
    elif key == 'c':
        print ('clear (not implemented)')

listen_keyboard(
    on_press=press,
)