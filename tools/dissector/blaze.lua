-- ==============================================================================
-- EA Blaze Protocol (Fire2 framing + Heat2 TDF codec) Wireshark Lua Dissector
-- Designed for FIFA 22 & Frostbite 3 Clean-Room Multiplayer Emulation
--
-- FIFA 22 speaks the 16-byte "Fire2" header with the "Heat2" payload codec.
-- The legacy 12-byte FIRE header and plain LEB128 varints decode *without
-- error* but with the wrong values, so every layout below cites the section
-- of docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md that pins it.
-- ==============================================================================

-- Wireshark's Lua engine supplies Proto / ProtoField / DissectorTable. The
-- codec further down is plain Lua, so that the self-test at the bottom of
-- this file can exercise it under a standalone interpreter:
--     lua -e "BLAZE_SELFTEST=true" tools/dissector/blaze.lua
local HAS_WIRESHARK = (type(Proto) == "function")

-- Component ID mappings (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §4.1)
local components = {
    [0x0001] = "Authentication",
    [0x0003] = "Example",
    [0x0004] = "GameManager",
    [0x0005] = "Redirector",
    [0x0007] = "Stats",
    [0x0009] = "Util",
    [0x000A] = "CensusData",
    [0x000B] = "Clubs",
    [0x000F] = "Messaging",
    [0x0019] = "AssociationLists",
    [0x001B] = "GpsContentController",
    [0x001C] = "GameReporting",
    [0x001F] = "ByteVault",
    [0x0021] = "Achievements",
    [0x0322] = "Packs",
    [0x0323] = "Inventory",
    [0x0325] = "PvzGw",
    [0x0401] = "XBLSystemConfigs",
    [0x0407] = "Friends",
    [0x1E7A] = "UserSessions"
}

-- Util command names (component 0x0009) -- docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §4.2
-- From EA's own UtilMessage enum. Command 0x0007 is preAuth, confirmed by
-- the two authentic EA preAuth replies kept in
-- crates/services-blaze/tests/fixtures/.
local util_commands = {
    [0x0001] = "fetchClientConfig",
    [0x0002] = "ping",
    [0x0003] = "setClientData",
    [0x0004] = "localizeStrings",
    [0x0005] = "getTelemetryServer",
    [0x0006] = "getTickerServer",
    [0x0007] = "preAuth",
    [0x0008] = "postAuth",
    [0x000A] = "userSettingsLoad",
    [0x000B] = "userSettingsSave",
    [0x000C] = "userSettingsLoadAll",
    [0x000D] = "userSettingsLoadAllForUserId",
    [0x0014] = "filterForProfanity",
    [0x0015] = "fetchQosConfig",
    [0x0016] = "setClientMetrics",
    [0x0017] = "setConnectionState"
}

-- Message Type mappings (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §2.1)
local msg_types = {
    [0x0] = "MESSAGE",
    [0x1] = "REPLY",
    [0x2] = "NOTIFICATION",
    [0x3] = "ERROR_REPLY",
    [0x4] = "PING",
    [0x5] = "PING_REPLY"
}

-- TDF Wire Type mappings (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.3)
-- 0x08 is OBJECT_TYPE and 0x09 is OBJECT_ID; 0x0A is FLOAT and 0x0B is
-- TIMEVALUE. 0x0C is GENERIC and 0x0D is the binary's unknown/default code.
local tdf_types = {
    [0x00] = "Integer",
    [0x01] = "String",
    [0x02] = "Blob",
    [0x03] = "Struct",
    [0x04] = "List",
    [0x05] = "Map",
    [0x06] = "Union",
    [0x07] = "Variable",
    [0x08] = "ObjectType",
    [0x09] = "ObjectId",
    [0x0A] = "Float",
    [0x0B] = "TimeValue",
    [0x0C] = "Generic",
    [0x0D] = "Unknown"
}

-- ==============================================================================
-- Heat2 codec -- plain Lua, no Wireshark dependency.
-- ==============================================================================

-- Formats an integral value without an exponent.
-- Lua 5.3 and later print a whole number as "1.792499378e+09", so the
-- dissector formats integers itself.
local function format_int(value)
    if value ~= math.floor(value) then
        return tostring(value)
    end
    if value < 2147483648 and value > -2147483648 then
        return string.format("%d", value)
    end
    return string.format("%.0f", value)
end

-- Reads one byte at a 0-based offset.
-- (Lua's string.byte is 1-based, and string.byte(s, 0) returns nil, so every
-- offset in this file is 0-based and goes through these helpers.)
local function byte_at(data, offset)
    return string.byte(data, offset + 1)
end

-- Reads `count` bytes at a 0-based offset.
local function bytes_at(data, offset, count)
    return string.byte(data, offset + 1, offset + count)
end

-- Reads `length` bytes at a 0-based offset as a string.
local function slice(data, offset, length)
    return string.sub(data, offset + 1, offset + length)
end

-- Decodes one 6-bit character group of a TDF tag.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.1)
-- A zero group terminates the name, so it decodes to an empty string.
local function decode_6bit(p)
    if p == 0 then return "" end
    if p >= 32 then
        -- Bit 5 set: an uppercase letter or symbol.
        return string.char((p % 32) + 64)
    end
    -- Bit 5 clear: a digit or a symbol.
    return string.char((p % 32) + 32)
end

-- Decodes a 3-byte packed TDF tag into its 1 to 4 ASCII characters.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.1)
-- FOUR characters are packed into three bytes, 6 bits each, at shifts
-- 18, 12, 6 and 0. A zero group ends the name.
local function decode_tag(b1, b2, b3)
    local tag = b1 * 65536 + b2 * 256 + b3
    local p0 = math.floor(tag / 262144) % 64
    local p1 = math.floor(tag / 4096) % 64
    local p2 = math.floor(tag / 64) % 64
    local p3 = tag % 64
    return decode_6bit(p0) .. decode_6bit(p1) .. decode_6bit(p2) .. decode_6bit(p3)
end

-- Reads a Heat2 "varsize" integer.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.2)
--   first byte:   bit 7 = continuation, bit 6 = negative, bits 0-5 = value
--   continuation: bit 7 = continuation, bits 0-6 = value
--   continuation value bits sit at shifts 6, 13, 20, 27, ...
-- This is NOT LEB128: a LEB128 first byte carries 7 value bits, Heat2 carries
-- 6 and reserves bit 6 for the sign, so the two schemes disagree on every
-- value >= 0x40.
--
-- Returns the magnitude, the sign flag and the number of bytes consumed, or
-- nil when the buffer runs out. The magnitude is a Lua number, so values
-- above 2^53 lose precision (Lua 5.1/5.2 have no 64-bit integers).
local function read_varsize(data, offset, limit)
    if offset > limit then return nil, nil, 0 end
    local first = byte_at(data, offset)
    if first == nil then return nil, nil, 0 end

    local negative = (math.floor(first / 64) % 2) == 1
    local magnitude = first % 64
    local consumed = 1

    if first >= 128 then
        -- Continuation: the next bytes carry 7 value bits each.
        local shift = 6
        while true do
            local next_offset = offset + consumed
            if next_offset > limit then return nil, nil, consumed end
            local b = byte_at(data, next_offset)
            if b == nil then return nil, nil, consumed end
            consumed = consumed + 1
            magnitude = magnitude + (b % 128) * (2 ^ shift)
            if b < 128 then break end
            shift = shift + 7
            if shift > 62 then return nil, nil, consumed end
        end
    end

    return magnitude, negative, consumed
end

-- Reads a Heat2 varsize integer and applies the sign.
local function read_varsize_signed(data, offset, limit)
    local magnitude, negative, consumed = read_varsize(data, offset, limit)
    if magnitude == nil then return nil, nil, 0 end
    if negative then return -magnitude, negative, consumed end
    return magnitude, negative, consumed
end

-- Decodes a 4-byte IEEE 754 big-endian float.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.4 -- FLOAT)
local function decode_float_be(data, offset)
    local b1, b2, b3, b4 = bytes_at(data, offset, 4)
    if b1 == nil then return nil end
    local sign = (b1 >= 128) and -1 or 1
    local exponent = (b1 % 128) * 2 + (b2 >= 128 and 1 or 0) - 127
    local mantissa = (b2 % 128) * 65536 + b3 * 256 + b4
    if exponent == 128 then
        if mantissa == 0 then return sign * math.huge end
        return 0 / 0 -- NaN
    end
    if exponent == -127 then
        return sign * mantissa * (2 ^ -149) -- subnormal
    end
    return sign * (1 + mantissa / 8388608) * (2 ^ exponent)
end

-- Big-endian integer readers for the Fire2 header.
local function be_u16(data, offset)
    local b1, b2 = bytes_at(data, offset, 2)
    if b2 == nil then return nil end
    return b1 * 256 + b2
end

local function be_u24(data, offset)
    local b1, b2, b3 = bytes_at(data, offset, 3)
    if b3 == nil then return nil end
    return b1 * 65536 + b2 * 256 + b3
end

local function be_u32(data, offset)
    local b1, b2, b3, b4 = bytes_at(data, offset, 4)
    if b4 == nil then return nil end
    return b1 * 16777216 + b2 * 65536 + b3 * 256 + b4
end

-- Decodes the 16-byte Fire2 header.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §2)
--   [0..4)   payloadSize   u32 BE -- the payload only; metadata is excluded
--   [4..6)   metadataSize  u16 BE -- the metadata length, NOT an error code
--   [6..8)   componentID   u16 BE
--   [8..10)  commandID     u16 BE
--   [10..13) messageNumber u24 BE
--   [13]     messageType << 5 | userIndex
--   [14..16) always zero -- there is no options byte in FIFA 22's Fire2
local function decode_fire2_header(data, offset)
    local payload_size = be_u32(data, offset)
    local metadata_size = be_u16(data, offset + 4)
    local component = be_u16(data, offset + 6)
    local command = be_u16(data, offset + 8)
    local message_number = be_u24(data, offset + 10)
    local flags = byte_at(data, offset + 13)
    local reserved = be_u16(data, offset + 14)
    if payload_size == nil or metadata_size == nil or component == nil
        or command == nil or message_number == nil or flags == nil then
        return nil
    end
    return {
        payload_size = payload_size,
        metadata_size = metadata_size,
        component = component,
        command = command,
        message_number = message_number,
        message_type = math.floor(flags / 32),
        user_index = flags % 32,
        reserved = reserved
    }
end

-- Decodes a TDF struct. Defined below; declared here because a nested struct
-- is a value of type 0x03.
local decode_tdf_struct

-- Decodes a TDF value of the given wire type.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.4)
-- Returns the decoded value and the number of bytes consumed, or nil when the
-- value cannot be decoded (an unknown wire type, or a truncated buffer).
local function decode_tdf_value(data, offset, limit, type_id)
    if type_id == 0x00 then
        -- INTEGER: varsize integer.
        local value, _, consumed = read_varsize_signed(data, offset, limit)
        if value == nil then return nil end
        return value, consumed
    elseif type_id == 0x01 then
        -- STRING: varsize length (which INCLUDES the NUL) + bytes + NUL.
        local length, _, vlen = read_varsize(data, offset, limit)
        if length == nil then return nil end
        local cursor = offset + vlen
        if cursor + length - 1 > limit then return nil end
        local raw = slice(data, cursor, length)
        -- The trailing NUL is part of the counted length but not of the value.
        local nul = string.find(raw, "\0", 1, true)
        if nul then raw = string.sub(raw, 1, nul - 1) end
        return raw, vlen + length
    elseif type_id == 0x02 then
        -- BLOB: varsize length + bytes.
        local length, _, vlen = read_varsize(data, offset, limit)
        if length == nil then return nil end
        local cursor = offset + vlen
        if cursor + length - 1 > limit then return nil end
        return slice(data, cursor, length), vlen + length
    elseif type_id == 0x03 then
        -- TDF: tagged fields + a 0x00 terminator.
        local fields, consumed = decode_tdf_struct(data, offset, limit, false)
        return { fields = fields }, consumed
    elseif type_id == 0x04 then
        -- LIST: element type byte + varsize count + elements.
        local item_type = byte_at(data, offset)
        if item_type == nil then return nil end
        local cursor = offset + 1
        local count, _, count_len = read_varsize(data, cursor, limit)
        if count == nil then return nil end
        cursor = cursor + count_len
        local items = {}
        for _ = 1, count do
            local value, vlen = decode_tdf_value(data, cursor, limit, item_type)
            if value == nil then
                return { item_type = item_type, count = count, items = items, truncated = true },
                    cursor - offset
            end
            items[#items + 1] = { value = value, start = cursor, stop = cursor + vlen }
            cursor = cursor + vlen
        end
        return { item_type = item_type, count = count, items = items }, cursor - offset
    elseif type_id == 0x05 then
        -- MAP: key type + value type + varsize count + pairs.
        local key_type = byte_at(data, offset)
        local val_type = byte_at(data, offset + 1)
        if key_type == nil or val_type == nil then return nil end
        local cursor = offset + 2
        local count, _, count_len = read_varsize(data, cursor, limit)
        if count == nil then return nil end
        cursor = cursor + count_len
        local entries = {}
        for _ = 1, count do
            local key, klen = decode_tdf_value(data, cursor, limit, key_type)
            if key == nil then
                return { key_type = key_type, val_type = val_type, count = count, entries = entries, truncated = true },
                    cursor - offset
            end
            local key_stop = cursor + klen
            local value, vlen = decode_tdf_value(data, key_stop, limit, val_type)
            if value == nil then
                return { key_type = key_type, val_type = val_type, count = count, entries = entries, truncated = true },
                    cursor - offset
            end
            entries[#entries + 1] = {
                key = key, key_start = cursor, key_stop = key_stop,
                value = value, value_start = key_stop, value_stop = key_stop + vlen
            }
            cursor = key_stop + vlen
        end
        return { key_type = key_type, val_type = val_type, count = count, entries = entries }, cursor - offset
    elseif type_id == 0x06 then
        -- UNION: case index + the member's OWN 3-byte tag + member type + value.
        local case_index = byte_at(data, offset)
        if case_index == nil then return nil end
        if case_index == 0xFF or case_index == 0x7F then
            -- An empty union carries no member.
            return { case = case_index, empty = true }, 1
        end
        local cursor = offset + 1
        local b1, b2, b3 = bytes_at(data, cursor, 3)
        if b3 == nil then return nil end
        local member_tag = decode_tag(b1, b2, b3)
        cursor = cursor + 3
        local member_type = byte_at(data, cursor)
        if member_type == nil then return nil end
        cursor = cursor + 1
        local value, vlen = decode_tdf_value(data, cursor, limit, member_type)
        if value == nil then return nil end
        return {
            case = case_index, member_tag = member_tag, member_type = member_type,
            value = value, value_start = cursor, value_stop = cursor + vlen
        }, cursor + vlen - offset
    elseif type_id == 0x07 then
        -- VARIABLE: present flag + varsize TDF id + member tag + type + value
        -- + the variable's own 0x00 terminator.
        local present = byte_at(data, offset)
        if present == nil then return nil end
        if present == 0 then
            return { present = false }, 1
        end
        local cursor = offset + 1
        local tdf_id, _, id_len = read_varsize(data, cursor, limit)
        if tdf_id == nil then return nil end
        cursor = cursor + id_len
        local b1, b2, b3 = bytes_at(data, cursor, 3)
        if b3 == nil then return nil end
        local member_tag = decode_tag(b1, b2, b3)
        cursor = cursor + 3
        local member_type = byte_at(data, cursor)
        if member_type == nil then return nil end
        cursor = cursor + 1
        local value, vlen = decode_tdf_value(data, cursor, limit, member_type)
        if value == nil then return nil end
        local value_stop = cursor + vlen
        local terminated = false
        if value_stop <= limit and byte_at(data, value_stop) == 0 then
            value_stop = value_stop + 1
            terminated = true
        end
        return {
            present = true, tdf_id = tdf_id, member_tag = member_tag, member_type = member_type,
            value = value, value_start = cursor, value_stop = value_stop, terminated = terminated
        }, value_stop - offset
    elseif type_id == 0x08 then
        -- OBJECT_TYPE: varsize component + varsize type. NOT a raw u16 pair.
        local component, _, clen = read_varsize(data, offset, limit)
        if component == nil then return nil end
        local cursor = offset + clen
        local object_type, _, tlen = read_varsize(data, cursor, limit)
        if object_type == nil then return nil end
        return { component = component, object_type = object_type }, clen + tlen
    elseif type_id == 0x09 then
        -- OBJECT_ID: varsize component + varsize type + varsize id.
        local component, _, clen = read_varsize(data, offset, limit)
        if component == nil then return nil end
        local cursor = offset + clen
        local object_type, _, tlen = read_varsize(data, cursor, limit)
        if object_type == nil then return nil end
        cursor = cursor + tlen
        local id, _, ilen = read_varsize(data, cursor, limit)
        if id == nil then return nil end
        return {
            component = component, object_type = object_type, id = id
        }, clen + tlen + ilen
    elseif type_id == 0x0A then
        -- FLOAT: 4 bytes, IEEE 754 big-endian.
        if offset + 3 > limit then return nil end
        local value = decode_float_be(data, offset)
        if value == nil then return nil end
        return value, 4
    elseif type_id == 0x0B then
        -- TIMEVALUE: varsize integer, microseconds.
        local value, _, consumed = read_varsize_signed(data, offset, limit)
        if value == nil then return nil end
        return value, consumed
    elseif type_id == 0x0C then
        -- GENERIC: a presence byte, then a varsize integer when present.
        local present = byte_at(data, offset)
        if present == nil then return nil end
        if present == 0 then
            return { present = false, value = 0 }, 1
        end
        local value, _, vlen = read_varsize(data, offset + 1, limit)
        if value == nil then return nil end
        return { present = true, value = value }, 1 + vlen
    end
    -- 0x0D (Unknown) and anything above it have no determinable length, so the
    -- caller stops and reports what it has.
    return nil
end

-- Decodes a TDF struct.
-- (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §3.4 and §3.5)
-- A *nested* struct ends with a 0x00 terminator; the *root* struct of a frame
-- does not -- the frame's length field delimits it. The metadata section of a
-- Fire2 frame is a root struct too.
--
-- Returns the list of fields, the number of bytes consumed and whether the
-- struct was cut short.
decode_tdf_struct = function(data, offset, limit, is_root)
    local fields = {}
    local cursor = offset
    local truncated = false

    while cursor < limit do
        local first = byte_at(data, cursor)
        if first == nil then
            truncated = true
            break
        end
        if first == 0x00 then
            if is_root then
                -- The root struct carries no terminator: a zero byte here is
                -- not part of the struct, so stop and leave it alone.
                break
            end
            return fields, cursor + 1 - offset, false
        end
        if cursor + 3 > limit then
            truncated = true
            break
        end
        local b1, b2, b3 = bytes_at(data, cursor, 3)
        local tag = decode_tag(b1, b2, b3)
        local type_id = byte_at(data, cursor + 3)
        if type_id == nil then
            truncated = true
            break
        end
        local value, vlen = decode_tdf_value(data, cursor + 4, limit, type_id)
        if value == nil then
            -- An unknown wire type or a truncated value: show the tag and the
            -- type, then stop -- the remaining bytes cannot be located.
            fields[#fields + 1] = {
                tag = tag, type_id = type_id, value = nil,
                start = cursor, stop = limit, undecodable = true
            }
            truncated = true
            break
        end
        fields[#fields + 1] = {
            tag = tag, type_id = type_id, value = value,
            start = cursor, stop = cursor + 4 + vlen
        }
        cursor = cursor + 4 + vlen
    end

    return fields, cursor - offset, truncated
end

-- ==============================================================================
-- Wireshark dissector
-- ==============================================================================
if HAS_WIRESHARK then
    local p_blaze = Proto("blaze", "EA Blaze Protocol")

    -- Header ProtoFields (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §2)
    local f_payload_size = ProtoField.uint32("blaze.payload_size", "Payload Size", base.DEC)
    local f_metadata_size = ProtoField.uint16("blaze.metadata_size", "Metadata Size", base.DEC)
    local f_comp = ProtoField.uint16("blaze.component", "Component", base.HEX, components)
    local f_cmd = ProtoField.uint16("blaze.command", "Command", base.HEX)
    local f_cmd_name = ProtoField.string("blaze.command_name", "Command Name")
    local f_msgno = ProtoField.uint24("blaze.msg_number", "Message Number", base.DEC)
    local f_msgtype = ProtoField.uint8("blaze.msg_type", "Message Type", base.HEX, msg_types)
    local f_userindex = ProtoField.uint8("blaze.user_index", "User Index", base.DEC)
    local f_reserved = ProtoField.uint16("blaze.reserved", "Reserved (always zero)", base.HEX)
    local f_warning = ProtoField.string("blaze.warning", "Warning")

    -- Payload ProtoFields
    local f_tdf_tag = ProtoField.string("blaze.tdf.tag", "Tag")
    local f_tdf_type = ProtoField.string("blaze.tdf.type", "Type")
    local f_tdf_val = ProtoField.string("blaze.tdf.value", "Value")
    local f_tdf_count = ProtoField.uint32("blaze.tdf.count", "Count", base.DEC)
    local f_tdf_item_type = ProtoField.string("blaze.tdf.item_type", "Item Type")
    local f_tdf_member = ProtoField.string("blaze.tdf.member_tag", "Member Tag")
    local f_tdf_case = ProtoField.uint8("blaze.tdf.case", "Case", base.DEC)

    p_blaze.fields = {
        f_payload_size, f_metadata_size, f_comp, f_cmd, f_cmd_name, f_msgno,
        f_msgtype, f_userindex, f_reserved, f_warning,
        f_tdf_tag, f_tdf_type, f_tdf_val, f_tdf_count, f_tdf_item_type, f_tdf_member, f_tdf_case
    }

    -- Renders a decoded TDF value into the tree.
    local render_value

    -- Renders one decoded TDF field (tag + type + value) into the tree.
    local function render_field(tree, wbuf, field)
        local type_name = tdf_types[field.type_id] or string.format("Unknown(0x%02X)", field.type_id)
        local node = tree:add(p_blaze, wbuf(field.start, field.stop - field.start),
            string.format("[%s] (%s)", field.tag, type_name))
        node:add(f_tdf_tag, field.tag)
        node:add(f_tdf_type, type_name)

        if field.undecodable then
            -- The wire type's length cannot be determined, so the rest of the
            -- struct cannot be located. Say so and stop.
            node:add(f_tdf_val, string.format("Undecodable value of type 0x%02X", field.type_id))
            return
        end

        render_value(node, wbuf, field.type_id, field.value, field.start + 4, field.stop)
    end

    -- Renders a decoded TDF value into the tree.
    render_value = function(tree, wbuf, type_id, value, vstart, vstop)
        if type_id == 0x00 then -- Integer
            tree:add(f_tdf_val, wbuf(vstart, vstop - vstart), format_int(value))
        elseif type_id == 0x01 then -- String
            tree:add(f_tdf_val, wbuf(vstart, vstop - vstart), value)
        elseif type_id == 0x02 then -- Blob
            tree:add(f_tdf_val, string.format("Blob (%d bytes)", #value))
        elseif type_id == 0x03 then -- Struct
            for _, child in ipairs(value.fields) do
                render_field(tree, wbuf, child)
            end
        elseif type_id == 0x04 then -- List
            tree:add(f_tdf_item_type, tdf_types[value.item_type] or string.format("0x%02X", value.item_type))
            tree:add(f_tdf_count, value.count)
            for index, item in ipairs(value.items) do
                local item_tree = tree:add(p_blaze, wbuf(item.start, item.stop - item.start),
                    string.format("[%d]", index))
                render_value(item_tree, wbuf, value.item_type, item.value, item.start, item.stop)
            end
        elseif type_id == 0x05 then -- Map
            tree:add(f_tdf_item_type, string.format("%s -> %s",
                tdf_types[value.key_type] or string.format("0x%02X", value.key_type),
                tdf_types[value.val_type] or string.format("0x%02X", value.val_type)))
            tree:add(f_tdf_count, value.count)
            for index, entry in ipairs(value.entries) do
                local entry_tree = tree:add(p_blaze, wbuf(entry.key_start, entry.value_stop - entry.key_start),
                    string.format("[%d]", index))
                render_value(entry_tree, wbuf, value.key_type, entry.key, entry.key_start, entry.key_stop)
                render_value(entry_tree, wbuf, value.val_type, entry.value, entry.value_start, entry.value_stop)
            end
        elseif type_id == 0x06 then -- Union
            tree:add(f_tdf_case, value.case)
            if value.empty then
                tree:add(f_tdf_val, "Empty union")
                return
            end
            tree:add(f_tdf_member, value.member_tag)
            tree:add(f_tdf_type, tdf_types[value.member_type] or string.format("0x%02X", value.member_type))
            render_value(tree, wbuf, value.member_type, value.value, value.value_start, value.value_stop)
        elseif type_id == 0x07 then -- Variable
            if not value.present then
                tree:add(f_tdf_val, "Variable not present")
                return
            end
            tree:add(f_tdf_val, string.format("TDF id %s", format_int(value.tdf_id)))
            tree:add(f_tdf_member, value.member_tag)
            tree:add(f_tdf_type, tdf_types[value.member_type] or string.format("0x%02X", value.member_type))
            render_value(tree, wbuf, value.member_type, value.value, value.value_start, value.value_stop)
        elseif type_id == 0x08 then -- ObjectType
            tree:add(f_tdf_val, string.format("component=%d type=%d", value.component, value.object_type))
        elseif type_id == 0x09 then -- ObjectId
            tree:add(f_tdf_val, string.format("component=%d type=%d id=%s",
                value.component, value.object_type, format_int(value.id)))
        elseif type_id == 0x0A then -- Float
            tree:add(f_tdf_val, wbuf(vstart, vstop - vstart), string.format("%g", value))
        elseif type_id == 0x0B then -- TimeValue
            tree:add(f_tdf_val, wbuf(vstart, vstop - vstart), string.format("%s us", format_int(value)))
        elseif type_id == 0x0C then -- Generic
            if not value.present then
                tree:add(f_tdf_val, "Generic value not present")
                return
            end
            tree:add(f_tdf_val, wbuf(vstart, vstop - vstart), string.format("Generic value %s", format_int(value.value)))
        end
    end

    -- Dissects one Fire2 frame: header, then the metadata section, then the
    -- payload (docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md §2.2).
    local function dissect_frame(data, offset, limit, subtree, wbuf)
        local header = decode_fire2_header(data, offset)
        if header == nil then return nil end

        local comp_name = components[header.component] or string.format("0x%04X", header.component)
        local cmd_name = string.format("0x%04X", header.command)
        if header.component == 0x0009 and util_commands[header.command] then
            cmd_name = util_commands[header.command]
        end
        local type_name = msg_types[header.message_type] or string.format("Unknown(%d)", header.message_type)

        local header_tree = subtree:add(p_blaze, wbuf(offset, 16), "Fire2 Header")
        header_tree:add(f_payload_size, wbuf(offset, 4), header.payload_size)
        header_tree:add(f_metadata_size, wbuf(offset + 4, 2), header.metadata_size)
        header_tree:add(f_comp, wbuf(offset + 6, 2), header.component)
        header_tree:add(f_cmd, wbuf(offset + 8, 2), header.command)
        header_tree:add(f_cmd_name, cmd_name)
        header_tree:add(f_msgno, wbuf(offset + 10, 3), header.message_number)
        header_tree:add(f_msgtype, wbuf(offset + 13, 1), header.message_type)
        header_tree:add(f_userindex, wbuf(offset + 13, 1), header.user_index)
        header_tree:add(f_reserved, wbuf(offset + 14, 2), header.reserved)
        if header.reserved ~= 0 then
            -- Bytes 14 and 15 are always zero in FIFA 22's Fire2: there is no
            -- options byte, and the serializer writes a zeroed 16-bit word.
            header_tree:add(f_warning,
                "Bytes 14-15 are not zero; FIFA 22's Fire2 always writes zero there")
        end

        -- The metadata section is present only when metadataSize > 0. It is
        -- an ordinary Heat2 TDF struct, written first, and like the payload it
        -- is a root struct: no trailing 0x00 terminator.
        local meta_start = offset + 16
        local meta_end = meta_start + header.metadata_size
        if header.metadata_size > 0 then
            local meta_tree = subtree:add(p_blaze, wbuf(meta_start, header.metadata_size), "Metadata")
            if meta_end > limit then
                meta_tree:add(f_tdf_val, "Truncated metadata section")
            else
                local fields = decode_tdf_struct(data, meta_start, meta_end, true)
                for _, field in ipairs(fields) do
                    render_field(meta_tree, wbuf, field)
                end
            end
        end

        local payload_start = meta_start + header.metadata_size
        local payload_end = payload_start + header.payload_size
        if header.payload_size > 0 then
            local payload_tree = subtree:add(p_blaze, wbuf(payload_start, header.payload_size), "TDF Payload")
            if payload_end > limit then
                payload_tree:add(f_tdf_val, "Truncated payload")
            else
                local fields = decode_tdf_struct(data, payload_start, payload_end, true)
                for _, field in ipairs(fields) do
                    render_field(payload_tree, wbuf, field)
                end
            end
        end

        local info = string.format("[%s] %s::%s (Msg: %s, Payload: %s, Metadata: %s)",
            type_name, comp_name, cmd_name, format_int(header.message_number),
            format_int(header.payload_size), format_int(header.metadata_size))
        local frame_len = 16 + header.metadata_size + header.payload_size

        return info, frame_len, header
    end

    -- Main Packet Dissector Function
    function p_blaze.dissector(buffer, pinfo, tree)
        local buf_len = buffer:len()
        if buf_len < 16 then
            return
        end

        pinfo.cols.protocol = "Blaze"

        local subtree = tree:add(p_blaze, buffer(), "EA Blaze Protocol Packet")
        local data = buffer(0, buf_len):string()

        local offset = 0
        local info = nil
        -- A TCP segment or a UDP datagram may hold more than one frame.
        while offset + 16 <= buf_len do
            local frame_info, frame_len = dissect_frame(data, offset, buf_len, subtree, buffer)
            if frame_info == nil then break end
            info = frame_info
            if frame_len == nil or frame_len <= 0 then break end
            offset = offset + frame_len
        end

        if info then
            pinfo.cols.info = info
        end
    end

    -- Register on the Blaze TCP and UDP ports (Redirector: 42127, Core: 42128)
    local tcp_port = DissectorTable.get("tcp.port")
    tcp_port:add(42127, p_blaze)
    tcp_port:add(42128, p_blaze)
    local udp_port = DissectorTable.get("udp.port")
    udp_port:add(42127, p_blaze)
    udp_port:add(42128, p_blaze)
end

-- ==============================================================================
-- Self-test -- the golden vectors from crates/blaze-tdf/tests/tdf_heat2_types.rs
-- and crates/blaze-tdf/tests/tdf_fire2_wire.rs.
-- Run it with:
--     lua -e "BLAZE_SELFTEST=true" tools/dissector/blaze.lua
-- ==============================================================================

-- Converts a hex string (spaces optional) into raw bytes.
local function hex_to_bytes(hex)
    local compact = string.gsub(hex, " ", "")
    return (string.gsub(compact, "%x%x", function(cc)
        return string.char(tonumber(cc, 16))
    end))
end

-- Flattens one decoded TDF field. Defined below; declared here because a
-- nested struct flattens its own fields.
local flatten_field

-- Flattens a decoded TDF value into a canonical one-line description.
local function flatten_value(type_id, value)
    if type_id == 0x00 then
        return format_int(value)
    elseif type_id == 0x01 then
        return string.format("\"%s\"", value)
    elseif type_id == 0x02 then
        return string.format("Blob(%d)", #value)
    elseif type_id == 0x03 then
        local parts = {}
        for _, field in ipairs(value.fields) do
            parts[#parts + 1] = flatten_field(field)
        end
        return "Struct{" .. table.concat(parts, ",") .. "}"
    elseif type_id == 0x04 then
        local parts = {}
        for _, item in ipairs(value.items) do
            parts[#parts + 1] = flatten_value(value.item_type, item.value)
        end
        return string.format("List(%s)[%s]", tdf_types[value.item_type], table.concat(parts, ","))
    elseif type_id == 0x05 then
        local parts = {}
        for _, entry in ipairs(value.entries) do
            parts[#parts + 1] = flatten_value(value.key_type, entry.key) .. "=" ..
                flatten_value(value.val_type, entry.value)
        end
        return string.format("Map(%s,%s)[%s]", tdf_types[value.key_type], tdf_types[value.val_type],
            table.concat(parts, ","))
    elseif type_id == 0x06 then
        if value.empty then
            return string.format("Union(case=%d,empty)", value.case)
        end
        return string.format("Union(case=%d,%s=%s)", value.case, value.member_tag,
            flatten_value(value.member_type, value.value))
    elseif type_id == 0x07 then
        if not value.present then
            return "Variable(absent)"
        end
        return string.format("Variable(present,id=%d,%s=%s)", value.tdf_id, value.member_tag,
            flatten_value(value.member_type, value.value))
    elseif type_id == 0x08 then
        return string.format("ObjectType(component=%d,type=%d)", value.component, value.object_type)
    elseif type_id == 0x09 then
        return string.format("ObjectId(component=%d,type=%d,id=%s)", value.component,
            value.object_type, format_int(value.id))
    elseif type_id == 0x0A then
        return string.format("Float(%.6f)", value)
    elseif type_id == 0x0B then
        return string.format("TimeValue(%s)", format_int(value))
    elseif type_id == 0x0C then
        return string.format("Generic(present=%s,value=%s)", tostring(value.present), format_int(value.value))
    end
    return "Unknown"
end

-- Flattens one decoded TDF field.
flatten_field = function(field)
    if field.undecodable then
        return string.format("%s:%s=<undecodable>", field.tag,
            tdf_types[field.type_id] or string.format("0x%02X", field.type_id))
    end
    return string.format("%s:%s=%s", field.tag,
        tdf_types[field.type_id] or string.format("0x%02X", field.type_id),
        flatten_value(field.type_id, field.value))
end

-- Decodes a hex string as a root TDF struct and flattens it.
local function flatten_payload(hex)
    local data = hex_to_bytes(hex)
    local fields = decode_tdf_struct(data, 0, #data, true)
    local parts = {}
    for _, field in ipairs(fields) do
        parts[#parts + 1] = flatten_field(field)
    end
    return table.concat(parts, ","), #data
end

local function run_self_test()
    local failures = 0
    local checks = 0

    local function check(label, got, expected)
        checks = checks + 1
        if got == expected then
            print(string.format("PASS  %s", label))
        else
            failures = failures + 1
            print(string.format("FAIL  %s\n        expected: %s\n        got:      %s",
                label, tostring(expected), tostring(got)))
        end
    end

    -- TDF tag unpacking: four 6-bit characters in three bytes (§3.1).
    local tags = {
        {"GID", "9E 99 00"},
        {"PCAP", "C2 38 70"},
        {"STIM", "CF 4A 6D"},
        {"CNTX", "8E ED 38"},
        {"ERRC", "97 2C A3"},
        {"FLAG", "9A C8 67"},
        {"LCTX", "B2 3D 38"},
        {"MADR", "B6 19 32"},
        {"MIID", "B6 9A 64"},
        {"NAME", "BA 1B 65"},
        {"SIDN", "CE 99 2E"},
        {"SKEY", "CE B9 79"}
    }
    for _, tag in ipairs(tags) do
        local bytes = {string.byte(hex_to_bytes(tag[2]), 1, 3)}
        check(string.format("tag %s", tag[1]), decode_tag(bytes[1], bytes[2], bytes[3]), tag[1])
    end

    -- Heat2 varsize integers (§3.2), the vectors from tdf_fire2_wire.rs.
    local varsize = {
        {0, "00", 1}, {1, "01", 1}, {2, "02", 1}, {62, "3E", 1}, {63, "3F", 1},
        {64, "80 01", 2}, {127, "BF 01", 2}, {128, "80 02", 2}, {255, "BF 03", 2},
        {256, "80 04", 2}, {16383, "BF FF 01", 3}, {16384, "80 80 02", 3},
        {1792499378, "B2 FA BA AD 0D", 5},
        {4294967296, "80 80 80 80 20", 5},
        {1099511627776, "80 80 80 80 80 40", 6},
        {-1, "41", 1}, {-2, "42", 1}, {-63, "7F", 1}, {-64, "C0 01", 2},
        {-65, "C1 01", 2}, {-1000, "E8 0F", 2}
    }
    for _, case in ipairs(varsize) do
        local data = hex_to_bytes(case[2])
        local value, _, consumed = read_varsize_signed(data, 0, #data)
        check(string.format("varsize %s", format_int(case[1])),
            string.format("%s/%d", format_int(value), consumed),
            string.format("%s/%d", format_int(case[1]), case[3]))
    end

    -- The Fire2 header of the authentic EA Util::ping request (§2.3).
    local ping = "00 00 00 00 00 00 00 09 00 02 00 00 00 00 00 00"
    local ping_data = hex_to_bytes(ping)
    local header = decode_fire2_header(ping_data, 0)
    check("ping header payload size", header.payload_size, 0)
    check("ping header metadata size", header.metadata_size, 0)
    check("ping header component", header.component, 0x0009)
    check("ping header command", header.command, 0x0002)
    check("ping header message number", header.message_number, 0)
    check("ping header message type", header.message_type, 0)
    check("ping header user index", header.user_index, 0)
    check("ping header reserved", header.reserved, 0)

    -- The Fire2 header of an authentic EA Util::preAuth reply.
    local reply = "00 00 05 6E 00 00 00 09 00 07 00 00 01 20 00 00"
    local reply_data = hex_to_bytes(reply)
    local reply_header = decode_fire2_header(reply_data, 0)
    check("reply header payload size", reply_header.payload_size, 1390)
    check("reply header component", reply_header.component, 0x0009)
    check("reply header command", reply_header.command, 0x0007)
    check("reply header message number", reply_header.message_number, 1)
    check("reply header message type", reply_header.message_type, 1)

    -- The TDF payload vectors from tdf_heat2_types.rs.
    local payloads = {
        {"integer 42", "DA 1B 35 00 2A", "VALU:Integer=42"},
        {"string hello", "BA 1B 65 01 06 68 65 6C 6C 6F 00", "NAME:String=\"hello\""},
        {"blob", "8A CB E2 02 04 DE AD BE EF", "BLOB:Blob=Blob(4)"},
        {"float 3.14", "C2 9F FF 0A 40 48 F5 C3", "PI__:Float=Float(3.140000)"},
        {"timevalue", "D2 9B 65 0B 80 AD E2 04", "TIME:TimeValue=TimeValue(5000000)"},
        {"object type", "BF 4E 70 08 04 01", "OTYP:ObjectType=ObjectType(component=4,type=1)"},
        {"object id", "BE 2A 64 09 04 01 A8 0F", "OBID:ObjectId=ObjectId(component=4,type=1,id=1000)"},
        {"list", "B2 9C F4 04 00 03 01 02 03", "LIST:List=List(Integer)[1,2,3]"},
        {"map", "B6 1C 3F 05 01 01 02 02 61 00 02 62 00 02 63 00 02 64 00",
            "MAP_:Map=Map(String,String)[\"a\"=\"b\",\"c\"=\"d\"]"},
        {"nested struct", "C6 FC F3 03 9E E8 6D 01 0C 52 6F 6F 6D 5F 58 59 37 39 4B 32 00 9F 4E 70 00 02 00",
            "QOSS:Struct=Struct{GNAM:String=\"Room_XY79K2\",GTYP:Integer=2}"},
        {"union", "D6 EA 6E 06 02 B6 2C BF 00 07", "UNIN:Union=Union(case=2,MBR_=7)"},
        {"variable", "DA 1C A9 07 01 92 13 DB 68 6C 00 A3 01 00",
            "VARI:Variable=Variable(present,id=1234,VVAL=99)"},
        {"absent variable", "C3 2D B4 07 00", "PRVT:Variable=Variable(absent)"},
        {"STIM", "CF 4A 6D 00 B2 FA BA AD 0D", "STIM:Integer=1792499378"},
        -- The 13-byte authentic EA capture: {GID: 1, PCAP: [2, 0]}. The root
        -- struct carries no trailing terminator (§3.5).
        {"EA capture", "9E 99 00 00 01 C2 38 70 04 00 02 02 00", "GID:Integer=1,PCAP:List=List(Integer)[2,0]"},
        -- GENERIC (0x0C) and the unknown/default code (0x0D) from §3.3.
        {"generic", "9E 5B B2 0C 01 2A", "GENR:Generic=Generic(present=true,value=42)"}
    }
    for _, case in ipairs(payloads) do
        local flattened, length = flatten_payload(case[2])
        check(string.format("payload %s", case[1]), flattened, case[3])
        check(string.format("payload %s length", case[1]), length,
            #hex_to_bytes(case[2]))
    end

    -- A 0x0D field stops the struct: its length cannot be determined.
    local unknown = flatten_payload("CF 4A 6D 0D")
    check("unknown type stops the struct", unknown, "STIM:Unknown=<undecodable>")

    print(string.format("\n%d checks, %d failures", checks, failures))
    if failures > 0 then
        if os ~= nil and os.exit ~= nil then os.exit(1) end
    end
    return failures
end

-- The self-test runs when it is asked for explicitly, so that loading the
-- dissector inside Wireshark does not run it.
local selftest_requested = false
if type(BLAZE_SELFTEST) == "boolean" then selftest_requested = BLAZE_SELFTEST end
if type(arg) == "table" and arg[1] == "--selftest" then selftest_requested = true end
if select("#", ...) > 0 and (... == "--selftest") then selftest_requested = true end
if selftest_requested then
    run_self_test()
end
