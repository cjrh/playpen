import sys
from datetime import datetime as dt
from datetime import timedelta

for line in sys.stdin:
    value = dt.fromisoformat(line.strip())
    output = value + timedelta(days=int(sys.argv[1]))
    print(output.isoformat())
