#!/usr/bin/env python

import os
import sys
import yaml
from shutil import copy2

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

prefix = 'ghcr.io/drogue-iot'
suffix = ':latest'


def is_drogue_image(image):
    if not image.startswith(prefix):
        return False
    if not image.endswith(suffix):
        return False
    return True


def translate_image(original):
    if org and original.startswith(prefix):
        original = org + original[len(prefix):]

    # unfortunately we cannot use .removesuffix yet, as this was added in Python 3.9, which
    # is too new for some distribution.
    if original.endswith(suffix):
        original = original[:-len(suffix)] + ":" + version

    return original


def replace_images(node):
    if isinstance(node, dict):
        if "image" in node:
            if is_drogue_image(node["image"]):
                node["image"] = translate_image(node["image"])
                node["imagePullPolicy"] = policy

        for key, value in node.items():
            replace_images(value)
    elif isinstance(node, list):
        for item in node:
            replace_images(item)


# Get the in_file and out_file for a directory entry
def to_files(p_root, p_name):
    file_in = os.path.join(p_root, p_name)
    if outdir:
        file_out = os.path.join(outdir, os.path.relpath(file_in, yamldir))
        os.makedirs(os.path.dirname(file_out), exist_ok=True)
    else:
        file_out = file_in
    return file_in, file_out


def process_yaml(file_in, file_out):

    newdocs = []
    with open(file_in) as f:
        docs = yaml.load_all(f, Loader=yaml.FullLoader)

        for doc in docs:
            replace_images(doc)
            newdocs.append(doc)

    with open(file_out, "w") as f:
        yaml.dump_all(newdocs, f)


for root, dirs, files in os.walk(yamldir, followlinks=True):
    for name in files:
        in_file, out_file = to_files(root, name)
        if name.endswith(".yaml"):
            print(f"Processing: {in_file} -> {out_file}")
            process_yaml(in_file, out_file)
        else:
            print(f"Copying: {in_file} -> {out_file}")
            copy2(in_file, out_file)
