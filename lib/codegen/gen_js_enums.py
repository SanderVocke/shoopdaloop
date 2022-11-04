import os, sys
pwd = os.path.dirname(__file__)
sys.path.append(pwd + '/..')
from LoopState import *
from pprint import *

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
    write_enum(f, 'LoopState', LoopState)
    write_enum(f, 'LoopActionType', LoopActionType)