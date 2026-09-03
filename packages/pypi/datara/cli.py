import sys
import subprocess
from . import _find_binary

def forgen_main():
    bin_path = _find_binary("forgen")
    sys.exit(subprocess.call([bin_path] + sys.argv[1:]))

def datara_main():
    bin_path = _find_binary("datara")
    sys.exit(subprocess.call([bin_path] + sys.argv[1:]))

def dpm_main():
    bin_path = _find_binary("dpm")
    sys.exit(subprocess.call([bin_path] + sys.argv[1:]))