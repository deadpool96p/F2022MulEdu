-- ==============================================================================
-- EA Blaze Protocol & Tagged Data Format (TDF) Wireshark Lua Dissector
-- Designed for FIFA 22 & Frostbite 3 Clean-Room Multiplayer Emulation
-- ==============================================================================

local p_blaze = Proto("blaze", "EA Blaze Protocol")

-- Component ID mappings
local components = {
    [0x0001] = "Authentication",
    [0x0004] = "GameManager",
    [0x0005] = "Redirector",
    [0x0007] = "Stats",
    [0x0009] = "Util",
    [0x000B] = "Census",
    [0x0019] = "Users",
    [0x7802] = "UserSessions"
}

-- Message Type mappings
local msg_types = {
    [0x0] = "SUBMIT_REQUEST",
    [0x1] = "RECORD_RESPONSE",
    [0x2] = "NOTIFICATION",
    [0x3] = "ERROR_RESPONSE"
}

-- TDF Wire Type mappings
local tdf_types = {
    [0x00] = "Integer",
    [0x01] = "String",
    [0x02] = "Blob",
    [0x03] = "Struct",
    [0x04] = "List",
    [0x05] = "Map",
    [0x06] = "Union",
    [0x07] = "VariableInteger",
    [0x08] = "ObjectId",
    [0x09] = "ObjectType",
    [0x0A] = "TimeValue",
    [0x0B] = "Float"
}

-- Header ProtoFields
local f_len      = ProtoField.uint24("blaze.length", "Payload Length", base.DEC)
local f_comp     = ProtoField.uint16("blaze.component", "Component", base.HEX, components)
local f_cmd      = ProtoField.uint16("blaze.command", "Command", base.HEX)
local f_err      = ProtoField.uint16("blaze.error", "Error Code", base.HEX)
local f_msgtype  = ProtoField.uint8("blaze.msg_type", "Message Type", base.HEX, msg_types, 0xF0)
local f_qos      = ProtoField.uint8("blaze.qos", "QoS / Context", base.HEX, nil, 0x0F)
local f_seqno    = ProtoField.uint16("blaze.seqno", "Sequence / Transaction ID", base.DEC)

-- Payload ProtoFields
local f_tdf_tag  = ProtoField.string("blaze.tdf.tag", "Tag")
local f_tdf_type = ProtoField.string("blaze.tdf.type", "Type")
local f_tdf_val  = ProtoField.string("blaze.tdf.value", "Value")

p_blaze.fields = {
    f_len, f_comp, f_cmd, f_err, f_msgtype, f_qos, f_seqno,
    f_tdf_tag, f_tdf_type, f_tdf_val
}

-- Bitwise helper compatibility (Wireshark 2.x - 4.x Lua bit / bit32)
local bit_op = bit or bit32

local function decode_6bit(p)
    if p == 0 then return "" end
    if bit_op.band(p, 0x20) ~= 0 then
        return string.char(bit_op.bor(bit_op.band(p, 0x1F), 0x40))
    else
        return string.char(bit_op.bor(bit_op.band(p, 0x1F), 0x20))
    end
end

local function decode_tag(b1, b2, b3)
    local tag = bit_op.bor(bit_op.lshift(b1, 16), bit_op.lshift(b2, 8), b3)
    local p0 = bit_op.band(bit_op.rshift(tag, 18), 0x3F)
    local p1 = bit_op.band(bit_op.rshift(tag, 12), 0x3F)
    local p2 = bit_op.band(bit_op.rshift(tag, 6), 0x3F)
    local p3 = bit_op.band(tag, 0x3F)
    return decode_6bit(p0) .. decode_6bit(p1) .. decode_6bit(p2) .. decode_6bit(p3)
end

local function read_varint(buffer, offset)
    local result = 0
    local shift = 0
    local len = 0
    while offset + len < buffer:len() do
        local b = buffer(offset + len, 1):uint()
        len = len + 1
        result = result + bit_op.lshift(bit_op.band(b, 0x7F), shift)
        if bit_op.band(b, 0x80) == 0 then
            break
        end
        shift = shift + 7
    end
    return result, len
end

-- Recursive TDF Struct Parser
local function dissect_tdf_struct(buffer, offset, max_len, parent_tree)
    local curr = offset
    local end_offset = offset + max_len

    while curr < end_offset do
        local first_byte = buffer(curr, 1):uint()
        if first_byte == 0x00 then
            -- TAG_END
            parent_tree:add(buffer(curr, 1), "[TAG_END]")
            curr = curr + 1
            break
        end

        if curr + 4 > buffer:len() then
            break
        end

        local b1 = buffer(curr, 1):uint()
        local b2 = buffer(curr + 1, 1):uint()
        local b3 = buffer(curr + 2, 1):uint()
        local tag_str = decode_tag(b1, b2, b3)
        local type_id = buffer(curr + 3, 1):uint()
        local type_name = tdf_types[type_id] or string.format("Unknown(0x%02X)", type_id)

        local field_tree = parent_tree:add(p_blaze, buffer(curr, 4), string.format("[%s] (%s)", tag_str, type_name))
        field_tree:add(f_tdf_tag, tag_str)
        field_tree:add(f_tdf_type, type_name)

        curr = curr + 4

        if type_id == 0x00 then -- Integer
            local val, vlen = read_varint(buffer, curr)
            field_tree:add(f_tdf_val, tostring(val))
            curr = curr + vlen
        elseif type_id == 0x01 then -- String
            local slen, vlen = read_varint(buffer, curr)
            curr = curr + vlen
            if slen > 0 and curr + slen <= buffer:len() then
                local str_val = buffer(curr, slen - 1):string()
                field_tree:add(f_tdf_val, str_val)
                curr = curr + slen
            end
        elseif type_id == 0x02 then -- Blob
            local blen, vlen = read_varint(buffer, curr)
            curr = curr + vlen
            if curr + blen <= buffer:len() then
                field_tree:add(f_tdf_val, string.format("Blob (%d bytes)", blen))
                curr = curr + blen
            end
        elseif type_id == 0x03 then -- Struct
            curr = dissect_tdf_struct(buffer, curr, end_offset - curr, field_tree)
        elseif type_id == 0x06 then -- Union
            if curr < buffer:len() then
                local active_tag = buffer(curr, 1):uint()
                curr = curr + 1
                if active_tag == 0xFF then
                    field_tree:add(f_tdf_val, "Empty Union (0xFF)")
                else
                    field_tree:add(f_tdf_val, string.format("Active Tag: 0x%02X", active_tag))
                    if curr < buffer:len() then
                        local inner_type = buffer(curr, 1):uint()
                        curr = curr + 1
                        -- read simple inner int/string
                        if inner_type == 0x00 or inner_type == 0x07 then
                            local val, vlen = read_varint(buffer, curr)
                            curr = curr + vlen
                            field_tree:add(f_tdf_val, tostring(val))
                        end
                    end
                end
            end
        else
            -- Skip primitive fallback
            if curr < buffer:len() then
                curr = curr + 1
            end
        end
    end

    return curr
end

-- Main Packet Dissector Function
function p_blaze.dissector(buffer, pinfo, tree)
    local buf_len = buffer:len()
    if buf_len < 12 then
        return
    end

    pinfo.cols.protocol = "Blaze"

    local subtree = tree:add(p_blaze, buffer(), "EA Blaze Protocol Packet")

    -- Parse 12-byte Blaze Header
    local header_tree = subtree:add(p_blaze, buffer(0, 12), "Blaze Header")
    header_tree:add(f_len, buffer(0, 3))
    header_tree:add(f_comp, buffer(3, 2))
    header_tree:add(f_cmd, buffer(5, 2))
    header_tree:add(f_err, buffer(7, 2))
    header_tree:add(f_msgtype, buffer(9, 1))
    header_tree:add(f_qos, buffer(9, 1))
    header_tree:add(f_seqno, buffer(10, 2))

    local comp_id = buffer(3, 2):uint()
    local cmd_id = buffer(5, 2):uint()
    local type_raw = bit_op.rshift(buffer(9, 1):uint(), 4)
    local comp_name = components[comp_id] or string.format("0x%04X", comp_id)
    local type_name = msg_types[type_raw] or "UNKNOWN"

    pinfo.cols.info = string.format("[%s] %s::0x%04X (Seq: %d)", type_name, comp_name, cmd_id, buffer(10, 2):uint())

    -- Dissect Payload if present
    local payload_len = buffer(0, 3):uint()
    if payload_len > 0 and buf_len >= 12 + payload_len then
        local payload_tree = subtree:add(p_blaze, buffer(12, payload_len), "Blaze TDF Payload")
        dissect_tdf_struct(buffer, 12, payload_len, payload_tree)
    end
end

-- Register on Blaze TCP Ports (Redirector: 42127, Core Server: 42128)
local tcp_port = DissectorTable.get("tcp.port")
tcp_port:add(42127, p_blaze)
tcp_port:add(42128, p_blaze)
