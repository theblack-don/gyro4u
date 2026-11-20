SHELL := /bin/bash

outdir ?= .
spec ?= packaging/fedora/gyro4u.spec

.PHONY: srpm
srpm:
	mkdir -p "$(outdir)"
	rpmbuild -bs \
		--define "_sourcedir $(CURDIR)" \
		--define "_srcrpmdir $(outdir)" \
		"$(spec)"
