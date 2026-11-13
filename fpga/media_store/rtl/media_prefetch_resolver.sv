module media_prefetch_resolver #(
    parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer BLOCK_CHUNKS = 1
) (
    input wire clk, input wire reset,
    input wire hint_valid, output wire hint_ready,
    input wire [63:0] hint_view, input wire [63:0] hint_position, input wire [63:0] hint_length,
    input wire [31:0] hint_count, input wire [31:0] view_count, input wire [31:0] slot_count,
    output wire descriptor_valid, input wire descriptor_ready, output wire [31:0] descriptor_address,
    input wire descriptor_response, input wire [159:0] descriptor_data,
    output wire table_valid, input wire table_ready, output wire [31:0] table_address,
    input wire table_response, input wire [31:0] table_data,
    output wire prefetch_valid, input wire prefetch_ready, output wire [31:0] prefetch_chunk
);
    localparam integer SHIFT = $clog2(CHUNK_FRAMES);
    localparam integer BLOCK_SHIFT = $clog2(BLOCK_CHUNKS);
    localparam integer SLOT_BITS = (SLOTS < 2) ? 1 : $clog2(SLOTS);
    typedef enum logic [2:0] { IDLE, DESCRIPTOR_REQUEST, DESCRIPTOR_WAIT, DESCRIPTOR_CAPTURE,
        TABLE_REQUEST, TABLE_WAIT, SEND } phase_t;
    phase_t phase;
    reg [31:0] view_index, remaining, first_offset, chunk;
    reg [63:0] cursor, length;
    reg [63:0] until_wrap;
    reg [31:0] span_contiguous, span;
    reg wraps;
    reg descriptor_valid_value;
    reg [31:0] descriptor_offset;
    reg [63:0] descriptor_frames;
    wire unused_descriptor = &{1'b0,descriptor_data[159:128],descriptor_data[31:1]};
    wire [63:0] absolute_sample = {32'b0,first_offset} + cursor;
    wire [31:0] chunk_left = 32'(CHUNK_FRAMES) - (absolute_sample[31:0] & 32'(CHUNK_FRAMES-1));
    wire [31:0] contiguous = remaining < chunk_left ? remaining : chunk_left;
    wire [31:0] planned_span = until_wrap < {32'b0,span_contiguous}
        ? until_wrap[31:0] : span_contiguous;
    assign hint_ready = phase == IDLE && !reset;
    assign descriptor_valid = phase == DESCRIPTOR_REQUEST && !reset;
    assign descriptor_address = view_index;
    assign table_valid = phase == TABLE_REQUEST && !reset;
    assign table_address = (view_index << SLOT_BITS) | 32'(absolute_sample >> (SHIFT+BLOCK_SHIFT));
    assign prefetch_valid = phase == SEND && !reset;
    assign prefetch_chunk = chunk;

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; view_index <= 0; remaining <= 0;
            first_offset <= 0; chunk <= 0; cursor <= 0; length <= 0;
            until_wrap <= 0; span_contiguous <= 0; span <= 0; wraps <= 0;
            descriptor_valid_value <= 0; descriptor_offset <= 0; descriptor_frames <= 0;
        end else case (phase)
            IDLE: if (hint_valid && hint_ready && hint_view < {32'b0,view_count}
                && hint_length != 0 && hint_position < hint_length && hint_count != 0) begin
                view_index <= hint_view[31:0]; cursor <= hint_position;
                length <= hint_length; remaining <= hint_count; phase <= DESCRIPTOR_REQUEST;
            end
            DESCRIPTOR_REQUEST: if (descriptor_ready) phase <= DESCRIPTOR_WAIT;
            DESCRIPTOR_WAIT: if (descriptor_response) begin
                descriptor_offset <= descriptor_data[63:32]; descriptor_frames <= descriptor_data[127:64];
                descriptor_valid_value <= descriptor_data[0]; phase <= DESCRIPTOR_CAPTURE;
            end
            DESCRIPTOR_CAPTURE: begin
                first_offset <= descriptor_offset;
                phase <= descriptor_valid_value && length <= descriptor_frames ? TABLE_REQUEST : IDLE;
            end
            TABLE_REQUEST: begin
                until_wrap <= length-cursor;
                span_contiguous <= contiguous;
                if (table_ready) phase <= TABLE_WAIT;
            end
            TABLE_WAIT: if (table_response) begin
                chunk <= (table_data << BLOCK_SHIFT) | (32'(absolute_sample >> SHIFT) & 32'(BLOCK_CHUNKS-1));
                span <= planned_span;
                wraps <= until_wrap <= {32'b0,span_contiguous};
                phase <= table_data < slot_count ? SEND : IDLE;
            end
            SEND: if (prefetch_ready) begin
                cursor <= wraps ? 64'b0 : cursor+{32'b0,span};
                remaining <= remaining-span;
                phase <= remaining <= span ? IDLE : TABLE_REQUEST;
            end
            default: phase <= IDLE;
        endcase
    end
endmodule
