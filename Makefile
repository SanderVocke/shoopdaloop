build/sooperlooper: FORCE
	cd third_party/sooperlooper && ./autogen.sh && ./configure && make
	mkdir -p build
	cp third_party/sooperlooper/src/sooperlooper build/

FORCE:

clean:
	rm -r build || true