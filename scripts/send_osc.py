import socket
import sys
import struct

# Credit for OSC message formatting is pynsmclient

class _OutgoingMessage(object):
    def __init__(self, oscpath):
        self.LENGTH = 4 #32 bit
        self.oscpath = oscpath
        self._args = []

    def write_string(self, val):
        dgram = val.encode('utf-8')
        diff = self.LENGTH - (len(dgram) % self.LENGTH)
        dgram += (b'\x00' * diff)
        return dgram

    def write_int(self, val):
        return struct.pack('>i', val)

    def write_float(self, val):
        return struct.pack('>f', val)

    def add_arg(self, argument):
        t = {str:"s", int:"i", float:"f"}[type(argument)]
        self._args.append((t, argument))

    def build(self):
        dgram = b''

        #OSC Path
        dgram += self.write_string(self.oscpath)

        if not self._args:
            dgram += self.write_string(',')
            return dgram

        # Write the parameters.
        arg_types = "".join([arg[0] for arg in self._args])
        dgram += self.write_string(',' + arg_types)
        for arg_type, value in self._args:
            f = {"s":self.write_string, "i":self.write_int, "f":self.write_float}[arg_type]
            dgram += f(value)
        return dgram

class _IncomingMessage(object):
    """Representation of a parsed datagram representing an OSC message.

    An OSC message consists of an OSC Address Pattern followed by an OSC
    Type Tag String followed by zero or more OSC Arguments.
    """

    def __init__(self, dgram):
        #NSM Broadcasts are bundles, but very simple ones. We only need to care about the single message it contains.
        #Therefore we can strip the bundle prefix and handle it as normal message.
        if b"#bundle" in dgram:
            bundlePrefix, singleMessage = dgram.split(b"/", maxsplit=1)
            dgram = b"/" + singleMessage  # / eaten by split
            self.isBroadcast = True
        else:
            self.isBroadcast = False
        self.LENGTH = 4 #32 bit
        self._dgram = dgram
        self._parameters = []
        self.parse_datagram()


    def get_int(self, dgram, start_index):
        """Get a 32-bit big-endian two's complement integer from the datagram.

        Args:
        dgram: A datagram packet.
        start_index: An index where the integer starts in the datagram.

        Returns:
        A tuple containing the integer and the new end index.

        Raises:
        ValueError if the datagram could not be parsed.
        """
        try:
            if len(dgram[start_index:]) < self.LENGTH:
                raise ValueError('Datagram is too short')
            return (
                struct.unpack('>i', dgram[start_index:start_index + self.LENGTH])[0], start_index + self.LENGTH)
        except (struct.error, TypeError) as e:
            raise ValueError('Could not parse datagram %s' % e)

    def get_string(self, dgram, start_index):
        """Get a python string from the datagram, starting at pos start_index.

        We receive always the full string, but handle only the part from the start_index internally.
        In the end return the offset so it can be added to the index for the next parameter.
        Each subsequent call handles less of the same string, starting further to the right.

        According to the specifications, a string is:
        "A sequence of non-null ASCII characters followed by a null,
        followed by 0-3 additional null characters to make the total number
        of bits a multiple of 32".

        Args:
        dgram: A datagram packet.
        start_index: An index where the string starts in the datagram.

        Returns:
        A tuple containing the string and the new end index.

        Raises:
        ValueError if the datagram could not be parsed.
        """
        #First test for empty string, which is nothing, followed by a terminating \x00 padded by three additional \x00.
        if dgram[start_index:].startswith(b"\x00\x00\x00\x00"):
            return "", start_index + 4

        #Otherwise we have a non-empty string that must follow the rules of the docstring.

        offset = 0
        try:
            while dgram[start_index + offset] != 0:
                offset += 1
            if offset == 0:
                raise ValueError('OSC string cannot begin with a null byte: %s' % dgram[start_index:])
            # Align to a byte word.
            if (offset) % self.LENGTH == 0:
                offset += self.LENGTH
            else:
                offset += (-offset % self.LENGTH)
            # Python slices do not raise an IndexError past the last index,
                # do it ourselves.
            if offset > len(dgram[start_index:]):
                raise ValueError('Datagram is too short')
            data_str = dgram[start_index:start_index + offset]
            return data_str.replace(b'\x00', b'').decode('utf-8'), start_index + offset
        except IndexError as ie:
            raise ValueError('Could not parse datagram %s' % ie)
        except TypeError as te:
            raise ValueError('Could not parse datagram %s' % te)

    def get_float(self, dgram, start_index):
        """Get a 32-bit big-endian IEEE 754 floating point number from the datagram.

          Args:
            dgram: A datagram packet.
            start_index: An index where the float starts in the datagram.

          Returns:
            A tuple containing the float and the new end index.

          Raises:
            ValueError if the datagram could not be parsed.
        """
        try:
            return (struct.unpack('>f', dgram[start_index:start_index + self.LENGTH])[0], start_index + self.LENGTH)
        except (struct.error, TypeError) as e:
            raise ValueError('Could not parse datagram %s' % e)

    def parse_datagram(self):
        try:
            self._address_regexp, index = self.get_string(self._dgram, 0)
            if not self._dgram[index:]:
                # No params is legit, just return now.
                return

            # Get the parameters types.
            type_tag, index = self.get_string(self._dgram, index)
            if type_tag.startswith(','):
                type_tag = type_tag[1:]

            # Parse each parameter given its type.
            for param in type_tag:
                if param == "i":  # Integer.
                    val, index = self.get_int(self._dgram, index)
                elif param == "f":  # Float.
                    val, index = self.get_float(self._dgram, index)
                elif param == "s":  # String.
                    val, index = self.get_string(self._dgram, index)
                else:
                    logger.warning("Unhandled parameter type: {0}".format(param))
                    continue
                self._parameters.append(val)
        except ValueError as pe:
            #raise ValueError('Found incorrect datagram, ignoring it', pe)
            # Raising an error is not ignoring it!
            logger.warning("Found incorrect datagram, ignoring it. {}".format(pe))

    @property
    def oscpath(self):
        """Returns the OSC address regular expression."""
        return self._address_regexp

    @staticmethod
    def dgram_is_message(dgram):
        """Returns whether this datagram starts as an OSC message."""
        return dgram.startswith(b'/')

    @property
    def size(self):
        """Returns the length of the datagram for this message."""
        return len(self._dgram)

    @property
    def dgram(self):
        """Returns the datagram from which this message was built."""
        return self._dgram

    @property
    def params(self):
        """Convenience method for list(self) to get the list of parameters."""
        return list(self)

    def __iter__(self):
        """Returns an iterator over the parameters of this message."""
        return iter(self._parameters)


def main():
    hostname = sys.argv[1]
    port = int(sys.argv[2])
    addr = sys.argv[3]
    args = sys.argv[4:]
    
    parsed_args = []
    for arg in args:
        elems = arg.split(':')
        if elems[0] == 's':
            parsed_args.append(str(elems[1]))
        elif elems[0] == 'i':
            parsed_args.append(int(elems[1]))
        elif elems[0] == 'f':
            parsed_args.append(float(elems[1]))
        else:
            print("Invalid argument type: {}".format(elems[0]))
            sys.exit(1)
    
    print("sending to {}:{}".format(hostname, port))
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(('', 0))
    ip, myport = sock.getsockname()
    
    msg = _OutgoingMessage(addr)
    for a in parsed_args:
        msg.add_arg(a)
    sock.sendto(msg.build(), (hostname, port))
    
    data, addr = sock.recvfrom(1024)
    response = _IncomingMessage(data)
    
    print(response.oscpath)
    print(response.params)
    
    
if __name__ == "__main__":
    main()