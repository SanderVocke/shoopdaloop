from docutils import nodes
from docutils.parsers.rst import Directive
import os
import re

script_path = os.path.dirname(os.path.realpath(__file__))
base_path = script_path + '/../../..'

class FunctionDocstrings(Directive):
    required_arguments = 1  
    
    def run(self):
        filename = self.arguments[0]
        full_filename = base_path + '/' + filename
        lines = []
        fns = []
        enums = []
        
        with open(full_filename, 'r') as f:
            lines = f.readlines()
        for idx, line in enumerate(lines):
            if re.match(r'^[ #/-]*@shoop_lua_fn_docstring.start', line):
                fn_lines = []
                for idx2 in range(idx+1, len(lines)):
                    if re.match(r'^[ #/-]*@shoop_lua_fn_docstring.end', lines[idx2]):
                        break
                    else:
                        fn_lines.append(lines[idx2].replace('#', '').replace('--', '').strip())
                signature = fn_lines[0]
                desc_lines = fn_lines[1:]
                fns.append({
                    'signature': signature,
                    'description': ' '.join(desc_lines)
                })
            elif re.match(r'^[ #/-]*@shoop_lua_enum_docstring.start', line):
                desc_lines = []
                for idx2 in range(idx+1, len(lines)):
                    if re.match(r'^[ #/-]*@shoop_lua_enum_docstring.end', lines[idx2]):
                        break
                    else:
                        desc_lines.append(lines[idx2].replace('#', '').replace('--', '').strip())
                enums.append({
                    'base': desc_lines[0],
                    'description': desc_lines[1],
                    'values': desc_lines[2:]
                })                
        
        root = nodes.paragraph()
        bullet_list = nodes.bullet_list()
        root += bullet_list
        for enum in enums:
            item = nodes.list_item()
            item += nodes.strong(text=enum['base']+'[{}]'.format(','.join(enum['values'])))
            item += nodes.paragraph(text=enum['description'])
            bullet_list += item
        for fn in fns:
            item = nodes.list_item()
            item += nodes.strong(text=fn['signature'])
            item += nodes.paragraph(text=fn['description'])
            bullet_list += item    
                
        return [root]
    
class LuaScriptDocstring(Directive):
    required_arguments = 1  
    
    def run(self):
        filename = self.arguments[0]
        full_filename = base_path + '/' + filename
        lines = None
        with open(full_filename, 'r') as f:
            lines = f.readlines()

        text = ''
        for line in lines:
            m = re.match(r'^\s*--\s*(.*)$', line)
            if m:
                text += m.group(1) + '\n'
            elif re.match(r'^\s*$', line):
                continue
            else:
                break
        
        root = nodes.paragraph()
        root += nodes.literal_block(text=text)
                
        return [root]

def setup(app):
    app.add_directive("shoop_function_docstrings", FunctionDocstrings)
    app.add_directive("shoop_lua_docstring", LuaScriptDocstring)

    return {
        'version': '1.0',
        'parallel_read_safe': True,
        'parallel_write_safe': True,
    }
