PANDOC ?= pandoc
WEASYPRINT ?= weasyprint
OUT ?= ../../../work/fpga-rtl-reference
HTML := $(OUT)/fpga-rtl-reference.html
PDF := $(OUT)/fpga-rtl-reference.pdf
SOURCES := manual.md opcodes.md
FIGURES := $(wildcard figures/*.svg)

.PHONY: all html pdf
all: html pdf
html: $(HTML)
pdf: $(PDF)

$(HTML): $(SOURCES) reference.css reference.html embed-figures.lua Makefile $(FIGURES)
	mkdir -p "$(OUT)"
	$(PANDOC) --from=markdown --to=html5 --standalone --embed-resources \
		--toc --toc-depth=1 --number-sections --css=reference.css \
		--template=reference.html --lua-filter=embed-figures.lua --no-highlight \
		--output="$@" $(SOURCES)

$(PDF): $(HTML)
	$(WEASYPRINT) "$<" "$@"
