# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

import os
import sys

sys.path.append(os.path.abspath("./_ext"))

project = 'ShoopDaLoop'
copyright = '2023, Sander Vocke'
author = 'Sander Vocke'

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = ['sphinxcontrib.fulltoc',
              'sphinxcontrib.plantuml',
              'shoop_docstrings']

templates_path = []
exclude_patterns = []
include_patterns = ['index.rst','developers.rst','usage.rst','concept.rst']



# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = 'sphinx_material'
html_static_path = ['_static']
html_theme_options = {
    'color_primary': 'grey',
    'color_accent': 'grey',
}

