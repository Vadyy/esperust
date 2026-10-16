import os
import subprocess


def run(*args, **kwargs):
    if os.name == 'nt':
        kwargs['creationflags'] = kwargs.get('creationflags', 0) | 0x08000000
    return subprocess.run(*args, **kwargs)
