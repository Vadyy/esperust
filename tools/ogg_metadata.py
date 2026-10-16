import struct

def vorbis_metadata(path):
    raw = path.read_bytes()
    offset, packet, rate, channels, last, serial = 0, bytearray(), None, None, 0, None
    while offset < len(raw):
        if raw[offset:offset + 5] != b'OggS\0' or offset + 27 > len(raw):
            raise ValueError('Malformed Ogg page: ' + str(path))
        granule, page_serial = struct.unpack_from('<QI', raw, offset + 6)
        if serial is None:
            serial = page_serial
        if page_serial != serial:
            raise ValueError('Chained/multiplexed Ogg is not supported: ' + str(path))
        count = raw[offset + 26]
        lacing = raw[offset + 27:offset + 27 + count]
        body = offset + 27 + count
        end = body + sum(lacing)
        if len(lacing) != count or end > len(raw):
            raise ValueError('Truncated Ogg page: ' + str(path))
        if rate is None:
            cursor = body
            for length in lacing:
                packet.extend(raw[cursor:cursor + length])
                cursor += length
                if length < 255:
                    if packet[:7] != b'\x01vorbis' or len(packet) < 30:
                        raise ValueError('Expected Vorbis identification header: ' + str(path))
                    channels, rate = packet[11], struct.unpack_from('<I', packet, 12)[0]
                    if not channels or not rate:
                        raise ValueError('Invalid Vorbis sample format')
                    break
        if granule != 0xffffffffffffffff:
            last = max(last, granule)
        offset = end
    if rate is None:
        raise ValueError('Missing Vorbis identification header')
    return {'codec_name': 'vorbis', 'sample_rate': rate, 'channels': channels, 'duration': last / rate}
