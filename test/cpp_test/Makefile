CC = g++
CFLAGS = -Wall -O3
TARGET = receive_test

all: $(TARGET)

$(TARGET): receive_test.c Makefile
	$(CC) $(CFLAGS) -o $(TARGET) receive_test.c

clean:
	rm -f receive_test

