#!/usr/bin/env python

import serial
import sys
import random

ser = serial.Serial(
	port='/dev/tty.usbmodemEB000001',
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
pkt_size = 256
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
num_of_pkts = 1024 * 32 
for i in range(0, num_of_pkts):
	vector = [ chr(random.randint(48,57)) for j in range(0,pkt_size)]
	if i % 2 == 0:
		vector[2] = chr(0x0F)
		vector[3] = chr(0xF0)
	else:
		vector[2] = chr(0xF0)
		vector[3] = chr(0x0F)

	string = "".join(vector)
	before = time.time() #time.clock()
	ser.write(string)
	read_string = ser.read(pkt_size)
	end = time.time() #time.clock()
	diff = end - before
	minimum = min(diff, minimum)
	maximum = max(diff, maximum)
        tot += diff 	

#	print ("DigitalInA({0}) DigitalInB({1})").format(bin(ord(read_string[0])), bin(ord(read_string[1])))
	if(string[2:] != read_string[2:]):
		print("Error cmp: index({0}) lenA({1}) lenB({2})").format(i, len(string), len(read_string))
		print ("A(%s) : B(%s)", string, read_string)
	
ser.close()
mean = tot/num_of_pkts
print ("Mean: %d:%d", mean, 1/mean )
print ("Min: %d:%d", minimum, 1/minimum)
print ("Max: %d:%d", maximum, 1/maximum)
print ("MB/s: %d", ((len(string) * num_of_pkts)/tot)/(1024*1024))

