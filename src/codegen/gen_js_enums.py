import os
from pprint import *
import sys
script_pwd = os.path.dirname(__file__)
sys.path.append(script_pwd + '/../shoopdaloop')

from lib.backend_wrappers import LoopMode, ChannelMode, AudioDriverType, PortDirection, FXChainType
from lib.types import KeyEventType

output_filename = sys.argv[1]

def write_enum(f, name, enum):
    f.write('const {} = {{'.format(name))
    for idx, i in enumerate(list(enum)):
        if i.name == 'names':
            continue
        f.write('{}\n  {}: {}'.format(
            ',' if idx > 0 else '',
            i.name,
            i.value
        ))
    f.write('\n}\n\n')
    if hasattr(enum, 'names'):
        f.write('const {}_names = {{'.format(name))
        for idx, i in enumerate(enum.names.value.items()):
            f.write('{}\n  {}.{}: {}'.format(
                ',' if idx > 0 else '',
                '[{}'.format(name),
                '{}]'.format(enum(i[0]).name),
                '"{}"'.format(i[1])
            ))
        f.write('\n}\n\n')

with open(output_filename, 'w') as f:
    write_enum(f, 'LoopMode', LoopMode)
    write_enum(f, 'ChannelMode', ChannelMode)
    write_enum(f, 'AudioDriverType', AudioDriverType)
    write_enum(f, 'PortDirection', PortDirection)
    write_enum(f, 'FXChainType', FXChainType)
    write_enum(f, 'KeyEventType', KeyEventType)