import os
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
RECREATION = Path(os.environ.get('ESPER_IMPORT_ROOT', TOOLS.parent)).resolve()
RESOURCES = Path(os.environ.get('ESPER_RESOURCES', RECREATION / '_resources')).resolve()
_index = os.environ.get('ESPER_RESOURCE_INDEX')
INDEX_PATH = Path(_index).resolve() if _index else None
if _index is None:
    for candidate in (RESOURCES.parent / 'resource.s4hd', RESOURCES.parent / 'resources.s4hd'):
        if candidate.is_file():
            INDEX_PATH = candidate
            break
OUTPUT = RECREATION / 'assets'
RESEARCH = RECREATION / 'research'
