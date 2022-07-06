# Makefile for rendering SVG from the .drawio files
#
# The drawio tool can be found at https://github.com/jgraph/drawio-desktop/releases
IMAGES := $(patsubst %.drawio,%.svg,$(wildcard *.drawio))

all: clean $(IMAGES)

%.svg: %.drawio
	drawio -x --embed-svg-images -e -f svg -o $@ $^

clean:
	rm -f *.svg

.PHONY: clean
