// One primitive loop's cut-time content and export ownership.
module media_cuts #(
    parameter integer CHANNELS=2, CONTENTS=8, EXPORTS=4
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready,
    input wire [2:0] request_opcode, input wire request_latest,
    input wire [31:0] request_slot, input wire [31:0] request_generation,
    input wire [831:0] request_state, input wire [63:0] request_frame, input wire [63:0] request_epoch,
    output reg response_valid, output reg [3:0] response_error,
    output wire [1727:0] response_data, output wire busy, output reg fault,
    output wire [31:0] capture_view0, output wire [31:0] capture_view1,
    output wire [31:0] playback_view0, output wire [31:0] playback_view1,
    output wire media_valid, input wire media_ready, output wire [3:0] media_opcode,
    output wire [63:0] media_view, output wire [63:0] media_length, input wire media_done,
    input wire [7:0] media_error, input wire [31:0] media_result_view
);
    localparam [31:0] NONE=32'hffffffff;
    localparam [3:0] CAPACITY=2, INVALID_ID=3, INVALID_STATE=5, EXHAUSTED=8;
    typedef enum reg [4:0] {INIT, IDLE, UPDATE_CAPTURE, FINALIZE, WRITE_DESCRIPTOR,
        EMPTY_UPDATE, FIND_PLAYBACK, FIND_WAIT, FIND_GET, START_CAPTURE,
        CREATE_SEND, CREATE_WAIT, SEAL_SEND, SEAL_WAIT, CLEANUP, ROOT_READ, ROOT_WAIT, ROOT_GET,
        REMOVE_SEND, REMOVE_WAIT, RESULT_READ, RESULT_WAIT, RESULT_GET, RESPOND} phase_t;
    phase_t phase;
    reg [2:0] operation;
    reg [31:0] cursor, selected, latest, playback, export_slot;
    reg [31:0] content_generation [0:CONTENTS-1];
    reg [31:0] export_generation [0:EXPORTS-1];
    reg [31:0] export_content [0:EXPORTS-1];
    reg [31:0] export_empty_generation [0:EXPORTS-1];
    reg [63:0] export_empty_revision [0:EXPORTS-1];
    reg [CONTENTS-1:0] content_valid;
    reg [EXPORTS-1:0] export_valid;
    reg [$clog2(EXPORTS+1)-1:0] references [0:CONTENTS-1];
    reg [31:0] empty_generation;
    reg [31:0] selected_empty_generation;
    reg [63:0] selected_empty_revision;
    reg [63:0] empty_revision, frame, epoch, bank0, bank1;
    reg [831:0] observed;
    reg capture_valid, next_bank, capture_bank;
    reg [63:0] capture_revision, capture_start, capture_frames, capture_base;
    reg [31:0] capture_root0, capture_root1, playback_root0, playback_root1;
    reg [63:0] roots;
    reg channel, loading_playback;
    reg [2:0] word_index;
    reg [511:0] descriptor;
    wire [63:0] descriptor_word;
    wire [63:0] selected_bank = capture_bank ? bank1 : bank0;
    wire [63:0] advanced_bank = capture_base + (observed[255:192] >> 5);
    wire [31:0] latest_generation = latest == NONE ? empty_generation : content_generation[latest];
    wire [31:0] playback_generation = playback == NONE ? empty_generation : content_generation[playback];
    wire descriptor_read = phase == FIND_PLAYBACK || phase == ROOT_READ || phase == RESULT_READ;
    wire [2:0] read_word = phase == FIND_PLAYBACK ? 3'd0 : phase == ROOT_READ ? 3'd5 : word_index;
    wire [31:0] read_slot = phase == FIND_PLAYBACK || phase == ROOT_READ ? cursor : selected;
    reg [63:0] write_word;
    always @* begin
        case (word_index)
            0: write_word=capture_revision;
            1: write_word=capture_frames;
            2: write_word=capture_start;
            3: write_word=frame;
            4: write_word=selected_bank;
            5: write_word={capture_root1,capture_root0};
            6: write_word={63'b0,capture_bank};
            default: write_word={observed[32:1],32'b0};
        endcase
    end
    sp_ram #(.WIDTH(64),.DEPTH(8*CONTENTS),.ADDR_BITS($clog2(8*CONTENTS))) descriptors (
        .clk(clk),.read_enable(descriptor_read),
        .read_address($clog2(8*CONTENTS)'(8*read_slot+32'(read_word))),.read_data(descriptor_word),
        .write_enable(phase == WRITE_DESCRIPTOR),
        .write_address($clog2(8*CONTENTS)'(8*selected+32'(word_index))),.write_data(write_word)
    );
    assign busy=phase != IDLE;
    assign request_ready=phase == IDLE && !fault && !reset;
    assign media_valid=phase == CREATE_SEND || phase == REMOVE_SEND || phase == SEAL_SEND;
    assign media_opcode=phase == CREATE_SEND ? 4'd0 : phase == SEAL_SEND ? 4'd11 : 4'd3;
    assign media_view=phase == CREATE_SEND ? 64'b0 : phase == SEAL_SEND
        ? {32'b0,channel ? capture_root1 : capture_root0} : {32'b0,channel ? roots[63:32] : roots[31:0]};
    assign media_length=phase == SEAL_SEND ? capture_frames : 64'b0;
    assign capture_view0=capture_valid ? capture_root0 : NONE;
    assign capture_view1=capture_valid ? capture_root1 : NONE;
    assign playback_view0=playback_root0;
    assign playback_view1=playback_root1;
    assign response_data={descriptor[511:448],selected == NONE ? selected_empty_generation : content_generation[selected],selected,
        descriptor[447:384],descriptor[319:256],descriptor[255:192],descriptor[191:128],
        descriptor[383:320],descriptor[127:64],selected == NONE ? selected_empty_revision : descriptor[63:0],
        export_slot == NONE ? 32'b0 : export_generation[export_slot],export_slot,
        playback_generation,playback,latest_generation,latest,epoch,frame,observed};
    task automatic finish(input [3:0] error);
        begin
            response_valid<=1; response_error<=error; phase<=IDLE;
            if (operation == 0 && error != 0 && phase != IDLE) fault<=1;
        end
    endtask
    task automatic result(input [31:0] slot);
        begin selected<=slot; word_index<=0; descriptor<=0; loading_playback<=0; phase<=slot == NONE ? RESPOND : RESULT_READ; end
    endtask
    task automatic next_cleanup;
        begin cursor<=cursor+1'b1; phase<=CLEANUP; end
    endtask
    initial if (CHANNELS < 1 || CHANNELS > 2 || CONTENTS < 1 || CONTENTS > 256 || EXPORTS < 1 || EXPORTS > 256)
        $fatal(1,"Invalid managed media profile");
    always @(posedge clk) begin : ownership
        integer i;
        reg [31:0] free_slot;
        response_valid<=0;
        if (reset) begin
            phase<=INIT; operation<=0; cursor<=0; selected<=NONE; latest<=NONE; playback<=NONE; export_slot<=NONE;
            content_valid<=0; export_valid<=0; empty_generation<=1; empty_revision<=0;
            selected_empty_generation<=1; selected_empty_revision<=0;
            frame<=0; epoch<=0; bank0<=0; bank1<=0; observed<=0; capture_valid<=0;
            next_bank<=0; capture_bank<=0; capture_revision<=0; capture_start<=0; capture_frames<=0; capture_base<=0;
            capture_root0<=NONE; capture_root1<=NONE; playback_root0<=NONE; playback_root1<=NONE;
            roots<=0; channel<=0; loading_playback<=0; word_index<=0; descriptor<=0; fault<=0; response_valid<=0; response_error<=0;
        end else if (!fault) case (phase)
            INIT: begin
                if (cursor < 32'(CONTENTS)) begin content_generation[cursor]<=1; references[cursor]<=0; end
                if (cursor < 32'(EXPORTS)) begin
                    export_generation[cursor]<=1; export_content[cursor]<=NONE;
                    export_empty_generation[cursor]<=0; export_empty_revision[cursor]<=0;
                end
                if (cursor+1 == 32'(CONTENTS > EXPORTS ? CONTENTS : EXPORTS)) begin cursor<=0; phase<=IDLE; end
                else cursor<=cursor+1'b1;
            end
            IDLE: if (request_valid) begin
                operation<=request_opcode; response_error<=0; export_slot<=NONE; selected<=NONE; descriptor<=0;
                if (request_opcode == 0) begin
                    observed<=request_state; frame<=request_frame; epoch<=request_epoch; phase<=UPDATE_CAPTURE;
                end else if (request_epoch != epoch) finish(INVALID_ID);
                else if (request_opcode == 1) begin
                    free_slot=NONE;
                    for (i=EXPORTS-1;i>=0;i=i-1) if (!export_valid[i]) free_slot=32'(i);
                    if (request_latest && latest == NONE) finish(INVALID_STATE);
                    else if (free_slot == NONE) finish(CAPACITY);
                    else begin
                        export_slot<=free_slot; export_valid[free_slot]<=1;
                        export_content[free_slot]<=request_latest ? latest : playback;
                        export_empty_generation[free_slot]<=empty_generation; export_empty_revision[free_slot]<=empty_revision;
                        selected_empty_generation<=empty_generation; selected_empty_revision<=empty_revision;
                        if ((request_latest ? latest : playback) != NONE)
                            references[request_latest ? latest : playback]<=references[request_latest ? latest : playback]+1'b1;
                        result(request_latest ? latest : playback);
                    end
                end else if (request_opcode == 2 || request_opcode == 3) begin
                    if (request_slot >= 32'(EXPORTS)) finish(INVALID_ID);
                    else if (!export_valid[request_slot] || export_generation[request_slot] != request_generation) finish(INVALID_ID);
                    else if (request_opcode == 3) begin
                        export_slot<=request_slot; selected_empty_generation<=export_empty_generation[request_slot];
                        selected_empty_revision<=export_empty_revision[request_slot]; result(export_content[request_slot]);
                    end
                    else if (&export_generation[request_slot]) finish(EXHAUSTED);
                    else begin
                        export_valid[request_slot]<=0; export_generation[request_slot]<=export_generation[request_slot]+1'b1;
                        if (export_content[request_slot] != NONE)
                            references[export_content[request_slot]]<=references[export_content[request_slot]]-1'b1;
                        cursor<=0; phase<=CLEANUP;
                    end
                end else finish(INVALID_STATE);
            end
            UPDATE_CAPTURE: begin
                if (capture_valid && capture_revision == observed[447:384]) begin
                    if (advanced_bank < capture_base) finish(EXHAUSTED);
                    else begin
                        capture_frames<=observed[255:192];
                        if (capture_bank) bank1<=advanced_bank; else bank0<=advanced_bank;
                        phase<=FINALIZE;
                    end
                end else phase<=FINALIZE;
            end
            FINALIZE: begin
                if (capture_valid && (observed[34:33] != 2 || capture_revision != observed[447:384])) begin
                    free_slot=NONE;
                    for (i=CONTENTS-1;i>=0;i=i-1) if (!content_valid[i]) free_slot=32'(i);
                    if (free_slot == NONE) finish(CAPACITY);
                    else begin selected<=free_slot; word_index<=0; channel<=0; phase<=SEAL_SEND; end
                end else phase<=EMPTY_UPDATE;
            end
            WRITE_DESCRIPTOR: begin
                if (&word_index) begin
                    content_valid[selected]<=1; references[selected]<=0; latest<=selected; capture_valid<=0;
                    phase<=EMPTY_UPDATE;
                end else word_index<=word_index+1'b1;
            end
            SEAL_SEND: if (media_ready) phase<=SEAL_WAIT;
            SEAL_WAIT: if (media_done) begin
                if (media_error != 0) finish(media_error == 3 ? CAPACITY : INVALID_STATE);
                else if (CHANNELS == 2 && !channel) begin channel<=1; phase<=SEAL_SEND; end
                else phase<=WRITE_DESCRIPTOR;
            end
            EMPTY_UPDATE: begin
                if (observed[127:64] == 0 && observed[34:33] != 2 && empty_revision != observed[383:320]) begin
                    if (&empty_generation) finish(EXHAUSTED);
                    else begin empty_generation<=empty_generation+1'b1; empty_revision<=observed[383:320]; latest<=NONE; cursor<=0; phase<=FIND_PLAYBACK; end
                end else begin cursor<=0; phase<=FIND_PLAYBACK; end
            end
            FIND_PLAYBACK: begin
                if (empty_revision == observed[383:320]) begin
                    playback<=NONE; playback_root0<=NONE; playback_root1<=NONE; phase<=START_CAPTURE;
                end else if (cursor == 32'(CONTENTS)) finish(INVALID_STATE);
                else phase<=FIND_WAIT;
            end
            FIND_WAIT: phase<=FIND_GET;
            FIND_GET: begin
                if (content_valid[cursor] && descriptor_word == observed[383:320]) begin
                    playback<=cursor; word_index<=0; selected<=cursor; descriptor<=0; loading_playback<=1; phase<=RESULT_READ;
                end else begin cursor<=cursor+1'b1; phase<=FIND_PLAYBACK; end
            end
            START_CAPTURE: begin
                if (observed[34:33] == 2 && !capture_valid) begin
                    if (&(next_bank ? bank1 : bank0)) finish(EXHAUSTED);
                    else begin
                        capture_valid<=1; capture_revision<=observed[447:384]; capture_bank<=next_bank;
                        capture_base<=(next_bank ? bank1 : bank0)+1'b1; capture_start<=frame; capture_frames<=observed[255:192];
                        if (next_bank) bank1<=bank1+1'b1; else bank0<=bank0+1'b1;
                        next_bank<=!next_bank; channel<=0; capture_root0<=NONE; capture_root1<=NONE; phase<=CREATE_SEND;
                    end
                end else begin cursor<=0; phase<=CLEANUP; end
            end
            CREATE_SEND: if (media_ready) phase<=CREATE_WAIT;
            CREATE_WAIT: if (media_done) begin
                if (media_error != 0) finish(media_error == 3 ? CAPACITY : INVALID_STATE);
                else begin
                    if (channel) capture_root1<=media_result_view; else capture_root0<=media_result_view;
                    if (CHANNELS == 2 && !channel) begin channel<=1; phase<=CREATE_SEND; end
                    else begin cursor<=0; phase<=CLEANUP; end
                end
            end
            CLEANUP: begin
                if (cursor == 32'(CONTENTS)) begin
                    selected_empty_generation<=empty_generation; selected_empty_revision<=empty_revision; result(playback);
                end
                else if (!content_valid[cursor] || cursor == latest || cursor == playback || references[cursor] != 0) cursor<=cursor+1'b1;
                else if (&content_generation[cursor]) finish(EXHAUSTED);
                else phase<=ROOT_READ;
            end
            ROOT_READ: phase<=ROOT_WAIT;
            ROOT_WAIT: phase<=ROOT_GET;
            ROOT_GET: begin roots<=descriptor_word; channel<=0; phase<=REMOVE_SEND; end
            REMOVE_SEND: if (media_ready) phase<=REMOVE_WAIT;
            REMOVE_WAIT: if (media_done) begin
                if (media_error != 0) finish(media_error == 3 ? CAPACITY : INVALID_STATE);
                else if (CHANNELS == 2 && !channel) begin channel<=1; phase<=REMOVE_SEND; end
                else begin content_valid[cursor]<=0; content_generation[cursor]<=content_generation[cursor]+1'b1; next_cleanup(); end
            end
            RESULT_READ: phase<=RESULT_WAIT;
            RESULT_WAIT: phase<=RESULT_GET;
            RESULT_GET: begin
                descriptor[64*word_index+:64]<=descriptor_word;
                if (&word_index) begin
                    if (loading_playback) begin
                        loading_playback<=0;
                        playback_root0<=descriptor[351:320]; playback_root1<=descriptor[383:352];
                        phase<=START_CAPTURE;
                    end else phase<=RESPOND;
                end else begin word_index<=word_index+1'b1; phase<=RESULT_READ; end
            end
            RESPOND: finish(0);
            default: begin fault<=1; finish(INVALID_STATE); end
        endcase
    end
endmodule
