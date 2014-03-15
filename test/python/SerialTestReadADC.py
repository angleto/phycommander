#!/usr/bin/env python

import serial
import sys
import random
import struct

ser = serial.Serial(
	port='/dev/tty.usbmodemfd131',
	#port='/dev/tty.usbmodemEB000001',
#	port='/dev/ttyACM0',
#	baudrate=5990400,
	parity=serial.PARITY_NONE,
	stopbits=serial.STOPBITS_ONE,
	bytesize=serial.EIGHTBITS
	)

#ser.open()

if ( not ser.isOpen() ):
	print ("Error opening device")
	sys.exit(1)

#pkt_size = 448
#pkt_size = 256
pkt_size = 64
#pkt_size = 512

#sample = "0123456789ABCDEF"
#string = ""
#for i in range(0,pkt_size/len(sample)):
#	string = sample + string

print pkt_size

minimum = 1000
maximum = 0
tot = 0

import time
num_of_pkts = 1024 * 1024
for i in range(0, num_of_pkts):
	before = time.time() #time.clock()
	read_string = ser.read(pkt_size)
	end = time.time() #time.clock()
	diff = end - before
	for i in zip(*(iter(read_string),) * 2):
		l = ''
		l += chr(ord(i[0]))
		l += chr(ord(i[1]))
		n = struct.unpack("H", l)
		diff = end - before
		minimum = min(diff, minimum)
		maximum = max(diff, maximum)
		tot += diff

ser.close()
mean = tot/num_of_pkts
print ("Mean: %d:%d", mean, 1/mean )
print ("Min: %d:%d", minimum, 1/minimum)
print ("Max: %d:%d", maximum, 1/maximum)
print ("MB/s: %d", ((pkt_size * num_of_pkts)/tot)/(1024*1024))
#print ("MB/s: %d", ((len(string) * num_of_pkts)/tot)/(1024*1024))

