OS = LINUX
#OS = MACOSX

ifeq ($(OS), LINUX)
CC = g++
CFLAGS = -Wall -O3 -DLINUX
TARGET = receive_test

else ifeq ($(OS), MACOSX)
CC = g++
CFLAGS = -Wall -O3 -DMACOSX
TARGET = receive_test
endif

all: $(TARGET)

$(TARGET): receive_test.c Makefile
	$(CC) $(CFLAGS) -o $(TARGET) receive_test.c

clean:
	rm -f receive_test

