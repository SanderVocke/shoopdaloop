module audio_configuration #(
    parameter integer KEYS=768,STATE_DEPTH=512
) (
    input wire clk,reset,
    input wire configuration_valid, output wire configuration_ready,
    input wire [7:0] configuration_opcode, input wire [639:0] configuration_data,
    output wire configuration_done, output wire [3:0] configuration_error, output wire [128:0] configuration_result,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire [511:0] request_data,
    output wire response_valid, output wire [511:0] response_data,
    input wire runtime_valid, output wire runtime_ready, input wire [511:0] runtime_data,
    output wire runtime_done, output wire [3:0] runtime_error, output wire [128:0] runtime_result,
    output wire busy,fault, output wire [63:0] graph_epoch,topology_epoch,output wire loaded,
    output wire [31:0] smoothing_frames,output wire smoothing_linear,
    output wire image_valid,input wire image_ready,output wire [7:0] image_opcode,
    output wire image_state,output wire [15:0] image_address,output wire [639:0] image_data,
    input wire image_done,input wire [3:0] image_error,input wire [128:0] image_result,
    output wire tp_valid,input wire tp_ready,output wire [7:0] tp_opcode,output wire [639:0] tp_data,
    input wire tp_done,input wire [3:0] tp_error,input wire [831:0] tp_result
);
    reg configuration_pending,configuration_stage,configuration_rejected;
    wire stages_ready,stage_configuration_ready,stage_configuration_done,stage_fault;
    wire [7:0] stage_configuration_error;
    wire stages_busy=!stages_ready;
    wire parameter_ready,parameter_busy,parameter_done,parameter_fault;
    wire [3:0] parameter_error;
    wire [128:0] parameter_result;
    wire activation_valid,activation_ready,activation_initial,activation_done,activation_fault;
    wire [7:0] activation_error;
    wire [63:0] activation_nonce;
    wire [191:0] activation_spec;
    wire ap_valid,ap_ready,ap_bank;
    wire [7:0] ap_opcode;
    wire [63:0] ap_nonce;
    wire ai_valid,ai_ready,ai_state;
    wire [7:0] ai_opcode;
    wire [639:0] ai_data;
    wire pi_valid,pi_ready,pi_state;
    wire [7:0] pi_opcode;
    wire [15:0] pi_address;
    wire [639:0] pi_data;
    wire common=stages_ready && !configuration_pending && !parameter_busy && !fault && !reset;
    wire known_configuration=configuration_opcode == 49 || configuration_opcode == 50;
    wire allowed_parameter=configuration_data[7:0] == 0 || configuration_data[7:0] == 1
        || configuration_data[7:0] == 2 || configuration_data[7:0] == 7 || configuration_data[7:0] == 8 || configuration_data[7:0] == 10;
    assign configuration_ready=common && !request_valid && !runtime_valid && known_configuration
        && (configuration_opcode == 50 ? stage_configuration_ready : parameter_ready);
    wire configuration_accept=configuration_valid && configuration_ready;
    assign request_ready=common && !configuration_valid && !runtime_valid;
    assign runtime_ready=common && !configuration_valid && !request_valid && parameter_ready;
    assign busy=stages_busy || configuration_pending || parameter_busy;
    assign fault=stage_fault || parameter_fault || activation_fault;
    assign configuration_done=configuration_pending && (configuration_rejected || (configuration_stage ? stage_configuration_done : parameter_done));
    assign configuration_error=configuration_rejected ? 4'd10 : configuration_stage ? (stage_configuration_error == 0 ? 4'd0 : 4'd5) : parameter_error;
    assign configuration_result=configuration_error != 0 ? 129'b0 : configuration_stage ? 129'b0 : parameter_result;
    assign runtime_done=parameter_done && !stages_busy && !configuration_pending;
    assign runtime_error=parameter_error;
    assign runtime_result=parameter_result;
    always @(posedge clk) begin
        if (reset) begin configuration_pending<=0;configuration_stage<=0;configuration_rejected<=0;end
        else begin
            if (configuration_done) configuration_pending<=0;
            if (configuration_accept) begin configuration_pending<=1;configuration_stage<=configuration_opcode == 50;
                configuration_rejected<=configuration_opcode == 49 && !allowed_parameter;end
        end
    end
    audio_stages stages (
        .clk(clk),.reset(reset),.configuration_valid(configuration_accept && configuration_opcode == 50),
        .configuration_ready(stage_configuration_ready),.configuration_opcode(configuration_data[639:632]),.configuration_data(configuration_data),
        .configuration_done(stage_configuration_done),.configuration_error(stage_configuration_error),
        .graph_epoch(graph_epoch),.topology_epoch(topology_epoch),.loaded(loaded),
        .request_valid(request_valid && request_ready),.request_ready(stages_ready),.request_opcode(request_opcode),.request_data(request_data),
        .response_valid(response_valid),.response_data(response_data),.fault(stage_fault),
        .activation_valid(activation_valid),.activation_ready(activation_ready),.activation_initial(activation_initial),
        .activation_nonce(activation_nonce),.activation_spec(activation_spec),.activation_done(activation_done),
        .activation_error(activation_error),.activation_fault(activation_fault)
    );
    wire [511:0] command=configuration_accept ? configuration_data[511:0] : runtime_data;
    wire parameter_valid=ap_valid || (configuration_accept && configuration_opcode == 49 && allowed_parameter) || (runtime_valid && runtime_ready);
    assign ap_ready=parameter_ready;
    audio_parameters #(.KEYS(KEYS),.STATE_DEPTH(STATE_DEPTH)) parameters (
        .clk(clk),.reset(reset),.initialization_open(!loaded),.request_valid(parameter_valid),.request_ready(parameter_ready),
        .request_opcode(stages_busy ? ap_opcode : command[7:0]),.request_key(stages_busy ? 16'b0 : command[23:8]),
        .request_bank(stages_busy ? ap_bank : command[24]),.request_nonce(stages_busy ? ap_nonce : command[95:32]),
        .request_count(stages_busy ? 32'b0 : command[127:96]),.request_address(stages_busy ? 16'b0 : command[143:128]),
        .request_value(stages_busy ? 129'b0 : command[272:144]),.request_target(stages_busy ? 32'b0 : command[304:273]),
        .request_duration(stages_busy ? 32'b0 : command[336:305]),.busy(parameter_busy),
        .response_valid(parameter_done),.response_error(parameter_error),.response_data(parameter_result),.fault(parameter_fault),
        .smoothing_frames(smoothing_frames),.smoothing_linear(smoothing_linear),
        .image_valid(pi_valid),.image_ready(pi_ready),.image_opcode(pi_opcode),.image_state(pi_state),.image_address(pi_address),.image_data(pi_data),
        .image_response_valid(image_done),.image_response_error(image_error),.image_response_data(image_result)
    );
    audio_activation activation (
        .clk(clk),.reset(reset),.request_valid(activation_valid),.request_ready(activation_ready),
        .request_initial(activation_initial),.request_nonce(activation_nonce),.request_spec(activation_spec),
        .response_valid(activation_done),.response_error(activation_error),.fault(activation_fault),
        .parameter_valid(ap_valid),.parameter_ready(ap_ready),.parameter_opcode(ap_opcode),.parameter_bank(ap_bank),.parameter_nonce(ap_nonce),
        .parameter_done(parameter_done),.parameter_error(parameter_error),
        .image_valid(ai_valid),.image_ready(ai_ready),.image_opcode(ai_opcode),.image_state(ai_state),.image_data(ai_data),
        .image_done(image_done),.image_error(image_error),
        .tp_valid(tp_valid),.tp_ready(tp_ready),.tp_opcode(tp_opcode),.tp_data(tp_data),.tp_done(tp_done),.tp_error(tp_error),.tp_result(tp_result)
    );
    assign image_valid=pi_valid || ai_valid;
    assign image_opcode=pi_valid ? pi_opcode : ai_opcode;
    assign image_state=pi_valid ? pi_state : ai_state;
    assign image_address=pi_valid ? pi_address : 16'b0;
    assign image_data=pi_valid ? pi_data : ai_data;
    assign pi_ready=image_ready;
    assign ai_ready=image_ready && !pi_valid;
    wire unused=&{1'b0,configuration_data[631:512],command[511:337],command[31:25]};
endmodule
