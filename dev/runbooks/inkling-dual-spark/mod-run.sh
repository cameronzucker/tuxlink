#!/bin/bash
set -e
echo "Applying Inkling streaming tool-call parser fix"
python3 "$(dirname "$0")/patch_inkling_parser.py" /usr/local/lib/python3.12/dist-packages/vllm/parser/inkling.py
