#!/usr/bin/env python3
"""Writes a C program that prints the value of every ParamEnums entry in Parameters.h."""
import re, sys
src = open(sys.argv[1]).read()
src = re.sub(r'/\*.*?\*/', '', src, flags=re.S); src = re.sub(r'//[^\n]*', '', src)
body = re.search(r'enum\s+ParamEnums\s*\{(.*?)\}', src, re.S).group(1)
names = [re.match(r'\s*([A-Za-z_]\w*)', e).group(1) for e in body.split(',') if re.match(r'\s*[A-Za-z_]', e)]
with open(sys.argv[2], 'w') as f:
    f.write('#include <stdio.h>\n#include "Parameters.h"\nint main(void){\n')
    for n in names:
        f.write(f'printf("%d\\t{n}\\n",(int){n});\n')
    f.write('return 0;}\n')
