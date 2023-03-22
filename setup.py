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
        'resampy',
        'mido',
        'jsonschema',
        'numpy',
        'pyjacklib'
      ],
      entry_points={
        'console_scripts': [
            'shoopdaloop = shoopdaloop.entry_points:main'
        ]
      },
      package_data={
        'shoopdaloop.lib.backend' : [
          'frontend_interface/*.so',
          'frontend_interface/*.py'
        ]
      })