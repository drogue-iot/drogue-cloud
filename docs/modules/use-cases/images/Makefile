IMAGES := $(patsubst %.drawio,%.svg,$(wildcard *.drawio))

all: clean $(IMAGES)

%.svg: %.drawio
	drawio -x --embed-svg-images -e -f svg -o $@ $^

clean:
	rm -f *.svg

.PHONY: clean
