#!/usr/bin/python
import sys
import os
import subprocess
import json
import re
import copy

ignore_regex = sys.argv[1]

all_ignores = set()
for file in sys.argv[2:]:
    tree_text = subprocess.check_output(['lddtree', '-a', file]).decode('utf-8')

    # Represent the lines with their depth
    tree_items = []
    for line in tree_text.split('\n')[1:]:
        if not line:
            continue
        depth = int((len(line) - len(line.lstrip()))/4)
        stripped = re.sub(r'[\s]*=>.*', '', line).strip()
        tree_items.append((depth, stripped))

    tree = {
        'name': 'root',
        'children': [],
        'parent': None,
        'depth': 0,
        'ignored': False
    }
    cur_parent = tree
    for depth, item in tree_items:
        while depth <= cur_parent['depth']:
            if cur_parent["parent"] is None:
                raise Exception('Unexpected depth decrease')
            cur_parent = cur_parent['parent']
        if depth > cur_parent['depth']+1:
            if depth > cur_parent['depth']+2:
                raise Exception('Unexpected depth increase')
            new_node = {
                'name': item,
                'children': [],
                'parent': cur_parent,
                'depth': cur_parent['depth']+2,
                'ignored': False
            }
            cur_parent = cur_parent['children'][-1]
            cur_parent['children'].append(new_node)
            continue
        
        new_node = {
            'name': item,
            'children': [],
            'parent': cur_parent,
            'depth': depth,
            'ignored': False
        }
        cur_parent['children'].append(new_node)

    def update_ignore(node):
        ignore_based_on_regex = bool(re.match(ignore_regex, node['name']))
        if (node['parent'] and node['parent']['ignored']) or ignore_based_on_regex:
            node['ignored'] = True
        for child in node['children']:
            update_ignore(child)

    update_ignore(tree)

    def print_node(node, depth=0):
        print('  '*depth + node['name'] + (' (ignored)' if node['ignored'] else ''))
        for child in node['children']:
            print_node(child, depth+1)
    # print_node(tree)

    def flatten(node, into, condition=lambda x: True):
        if condition(node):
            into.add(node['name'])
        for child in node['children']:
            flatten(child, into, condition)

    non_ignore_set = set()
    flatten(tree, non_ignore_set, lambda node: not node['ignored'])
    non_ignore_set.remove('root')

    ignore_set = set()
    flatten(tree, ignore_set, lambda node: node['ignored'])

    final_ignore_set = {i for i in ignore_set if i not in non_ignore_set}
    all_ignores = all_ignores.union(final_ignore_set)

print(','.join([re.sub(r'\.so\..*', '.so', e) for e in all_ignores]).replace('\n', '').replace('\r', '').strip())