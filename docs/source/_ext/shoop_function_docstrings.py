from docutils import nodes
from docutils.parsers.rst import Directive
import os
import imp
import pydoc
import re

script_path = os.path.dirname(os.path.realpath(__file__))
base_path = script_path + '/../../..'

class FunctionDocstrings(Directive):
    required_arguments = 2     
    
    def run(self):
        filename = self.arguments[0]
        entity = self.arguments[1]
        full_filename = base_path + '/' + filename
        lines = []
        rval = []
        fns = []
        enums = []
        
        with open(full_filename, 'r') as f:
            lines = f.readlines()
        for idx, line in enumerate(lines):
            if re.match(r'^[ #/]*@shoop_lua_fn_docstring.start', line):
                fn_lines = []
                for idx2 in range(idx+1, len(lines)):
                    if re.match(r'^[ #/]*@shoop_lua_fn_docstring.end', lines[idx2]):
                        break
                    else:
                        fn_lines.append(lines[idx2].replace('#', '').replace('--', '').strip())
                signature = fn_lines[0]
                desc_lines = fn_lines[1:]
                fns.append({
                    'signature': signature,
                    'description': ' '.join(desc_lines)
                })
            elif re.match(r'^[ #/]*@shoop_lua_enum_docstring.start', line):
                desc_lines = []
                for idx2 in range(idx+1, len(lines)):
                    if re.match(r'^[ #/]*@shoop_lua_enum_docstring.end', lines[idx2]):
                        break
                    else:
                        desc_lines.append(lines[idx2].replace('#', '').replace('--', '').strip())
                rval.append(nodes.paragraph(text=' '.join(desc_lines)))
                
        return [nodes.paragraph(text='Shoopy hi shoopy.')]

def setup(app):
    app.add_directive("shoop_function_docstrings", FunctionDocstrings)

    return {
        'version': '1.0',
        'parallel_read_safe': True,
        'parallel_write_safe': True,
    }