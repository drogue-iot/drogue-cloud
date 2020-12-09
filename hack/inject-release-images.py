#!/usr/bin/env python

import os
import sys
import yaml

version = sys.argv[1]
policy = sys.argv[2]
yamldir = sys.argv[3]

print(f"Inject tag: {version} ({policy}) for files in {yamldir}")


def translate_image(original):
    if not original.endswith(":latest"):
        return original

    # unfortunately we cannot use .removesuffix yet, as this was added in Python 3.9, which
    # is too new for some distribution.
    suffix = ':latest'
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


def process_yaml(file_name):
    print(f"Processing: {file_name}")

    newdocs = []
    with open(file_name) as f:
        docs = yaml.load_all(f, Loader=yaml.FullLoader)

        for doc in docs:
            replace_images(doc)
            newdocs.append(doc)

    with open(file_name, "w") as f:
        yaml.dump_all(newdocs, f)


for root, dirs, files in os.walk(yamldir):
    for name in files:
        process_yaml(os.path.join(root, name))
