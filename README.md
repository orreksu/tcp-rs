# High-level approach
1. Get the sender and receiver correctly transferring data. We implemented
   the 'byte encoding' straight away since it was pretty much easier to do.
2. Then we added code to implement reliability (retransmissions + FIN packets).
3. Then we rewrote everything to better represent data, and write existing 
   code in a better and more extensible way.
4. Then we implemented the 'basic window' that just sends n packet over the net.
5. Next, we added realibility to the program.

# Challenges
The main challenge was making design for the program. We initially did it incorrectly
(not enough data types and a lot of looping with conditional breaks),
which made programming extremely annoying and time-consuming. After the first rewrite
(after implementing basic protocol without reliability), which made further coding much
more easier and enjoyable.

# Testing
After implementing each impovemnet we tested it by specifiying required options in netsim.
Than we ran the testall multiple times and also tested various additional scenarios with 
netsim + nettest.
