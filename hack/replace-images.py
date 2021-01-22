#!/usr/bin/env python

import os
import sys
import yaml

# <version> <policy> <dir> [<org> [<outdir>]]

version = sys.argv[1]
policy = sys.argv[2]
yamldir = sys.argv[3]
org = None
if len(sys.argv) > 4:
    org = sys.argv[4]
outdir = None
if len(sys.argv) > 5:
    outdir = sys.argv[5]

print(f"Inject tag: {version} ({policy}) for files in {yamldir}")


def translate_image(original):

    prefix = 'ghcr.io/drogue-iot'
    suffix = ':latest'

    if not original.startswith(prefix):
        return original
    if not original.endswith(suffix):
        return original

    if org and original.startswith(prefix):
        original = org + original[len(prefix):]

    # unfortunately we cannot use .removesuffix yet, as this was added in Python 3.9, which
    # is too new for some distribution.
    if original.endswith(suffix):
        original = original[:-len(suffix)] + ":" + version

    return original


def replace_images(node):
    if isinstance(node,dict):
        if "image" in node:
            node["image"] = translate_image(node["image"])
            node["imagePullPolicy"] = policy

        for key, value in node.items():
            replace_images(value)
    elif isinstance(node, list):
        for item in node:
            replace_images(item)


def process_yaml(p_root, p_name):
    in_file = os.path.join(p_root, p_name)
    if outdir:
        out_file = os.path.join(outdir, os.path.relpath(in_file, yamldir))
        os.makedirs(os.path.dirname(out_file), exist_ok=True)
        print(f"Processing: {in_file} -> {out_file}")
    else:
        out_file = in_file
        print(f"Processing: {in_file}")

    newdocs = []
    with open(in_file) as f:
        docs = yaml.load_all(f, Loader=yaml.FullLoader)

        for doc in docs:
            replace_images(doc)
            newdocs.append(doc)

    with open(out_file, "w") as f:
        yaml.dump_all(newdocs, f)


for root, dirs, files in os.walk(yamldir):
    for name in files:
        if name.endswith(".yaml"):
            process_yaml(root, name)
