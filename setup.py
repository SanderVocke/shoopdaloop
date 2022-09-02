from setuptools import setup, find_packages

setup(name='shoopdaloop',
      version='0.1',
      description='Loop-based music creation tool for live use or recording',
      url='https://gitea.octiron.soleus.nu/sander/shoopdaloop',
      author='Sander Vocke',
      author_email='sandervocke@gmail.com',
      license='MIT',
      packages=find_packages(exclude = 'codegen'),
      zip_safe=False,
      install_requires=[
        'playsound',
        'soundfile',
        'resampy',
        'mido',
        'jsonschema',
        'numpy',
        'pyjacklib',
        'pyside6'
      ],
      entry_points={
        'console_scripts': [
            'shoopdaloop = shoopdaloop.__main__:main'
        ]
      },
      package_data={
        'shoopdaloop.lib.backend' : [
          'frontend_interface/*.so',
          'frontend_interface/*.py',
          'frontend_interface/*.js'
        ],
        'shoopdaloop.lib' : [
          'qml/*',
          'qml/applications/*',
          'qml/test/*',
          '*.js',
          'session_schemas/*',
          'session_schemas/audioport/*',
          'session_schemas/channel/*',
          'session_schemas/loop/*',
          'session_schemas/midiport/*',
          'session_schemas/session/*',
          'session_schemas/track/*',
        ],
        'shoopdaloop' : [
          'third_party/QtMaterialDesignIcons/*',
          'third_party/QtMaterialDesignIcons/qml/*',
          'third_party/QtMaterialDesignIcons/resources/*',
          'third_party/QtMaterialDesignIcons/sources/*',
          'resources/*'
        ]
      })