.PHONY: all clean

all: herdr-nvim-aware

herdr-nvim-aware: herdr-nvim-aware.c
	cc -O2 -Wall -Wextra -o $@ $<

clean:
	rm -f herdr-nvim-aware
