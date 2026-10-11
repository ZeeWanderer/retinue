%%%-------------------------------------------------------------------
%% Minimal MCP 2025-11-25 stdio server with asynchronous tool calls.
%%%-------------------------------------------------------------------
-module(retinue_mcp_server).

-export([run/1, request/2]).

-define(LATEST_PROTOCOL, <<"2025-11-25">>).

-type state() :: #{pending := map(), monitors := map(), protocol := binary()}.

-spec run(module()) -> ok | {error, term()}.
run(Adapter) ->
    process_flag(trap_exit, true),
    Parent = self(),
    Reader = spawn_link(fun() -> read_loop(Parent) end),
    loop(#{pending => #{}, monitors => #{}, protocol => ?LATEST_PROTOCOL,
           reader => Reader, adapter => Adapter}).

-doc "Handle one side-effect-free MCP request; used by tests and immediate methods.".
-spec request(module(), map()) -> {ok, map()} | {error, integer(), binary(), term()} | async.
request(_Adapter, #{<<"params">> := Params}) when not is_map(Params) ->
    {error, -32602, <<"Invalid params">>, object_required};
request(Adapter, #{<<"method">> := <<"initialize">>} = Message) ->
    Params = maps:get(<<"params">>, Message, #{}),
    Requested = maps:get(<<"protocolVersion">>, Params, ?LATEST_PROTOCOL),
    Protocol = negotiate_protocol(Requested),
    {ok, #{<<"protocolVersion">> => Protocol,
           <<"capabilities">> =>
               #{<<"tools">> => #{<<"listChanged">> => false},
                 <<"resources">> => #{<<"subscribe">> => false,
                                        <<"listChanged">> => false}},
           <<"serverInfo">> =>
               #{<<"name">> => Adapter:name(), <<"version">> => app_version(),
                 <<"description">> => <<"Structured MCP access to retinue-daemon">>},
           <<"instructions">> =>
               Adapter:instructions()}};
request(_Adapter, #{<<"method">> := <<"ping">>}) ->
    {ok, #{}};
request(Adapter, #{<<"method">> := <<"tools/list">>}) ->
    {ok, #{<<"tools">> => Adapter:definitions()}};
request(Adapter, #{<<"method">> := <<"resources/list">>}) ->
    {ok, #{<<"resources">> => Adapter:resources()}};
request(Adapter, #{<<"method">> := <<"resources/read">>, <<"params">> := Params})
  when is_map(Params) ->
    case maps:get(<<"uri">>, Params, undefined) of
        Uri when is_binary(Uri) ->
            case Adapter:read(Uri) of
                {ok, MimeType, Text} ->
                    {ok, #{<<"contents">> =>
                               [#{<<"uri">> => Uri, <<"mimeType">> => MimeType,
                                  <<"text">> => Text}]}};
                {error, Reason} ->
                    {error, -32002, <<"Resource not found">>, Reason}
            end;
        _ -> {error, -32602, <<"Invalid params">>, uri_required}
    end;
request(_Adapter, #{<<"method">> := <<"tools/call">>}) ->
    async;
request(_Adapter, #{<<"method">> := Method}) ->
    {error, -32601, <<"Method not found">>, Method};
request(_Adapter, _Message) ->
    {error, -32600, <<"Invalid Request">>, invalid_request}.

-spec loop(state()) -> ok | {error, term()}.
loop(State) ->
    receive
        {mcp_line, Line} ->
            loop(handle_line(Line, State));
        {tool_result, Pid, Id, Result} ->
            loop(complete_tool(Pid, Id, Result, State));
        {'DOWN', Monitor, process, _Pid, Reason} ->
            loop(tool_down(Monitor, Reason, State));
        {mcp_reader_error, Reason} ->
            cancel_all(State),
            {error, {stdio_read_failed, Reason}};
        mcp_eof ->
            cancel_all(State),
            ok;
        {'EXIT', Reader, Reason} when Reader =:= map_get(reader, State), Reason =/= normal ->
            cancel_all(State),
            {error, {stdio_reader_down, Reason}};
        {'EXIT', _Pid, _Reason} ->
            loop(State)
    end.

handle_line(Line, State) ->
    case retinue_json:decode(Line) of
        {ok, Message} when is_map(Message) -> dispatch(Message, State);
        {ok, _Other} ->
            send_error(null, -32600, <<"Invalid Request">>, invalid_request),
            State;
        {error, Reason} ->
            send_error(null, -32700, <<"Parse error">>, Reason),
            State
    end.

dispatch(#{<<"jsonrpc">> := <<"2.0">>, <<"method">> := Method} = Message, State)
  when is_binary(Method) ->
    case maps:find(<<"id">>, Message) of
        error -> handle_notification(Method, maps:get(<<"params">>, Message, #{}), State);
        {ok, Id} -> handle_request(Id, Message, State)
    end;
dispatch(Message, State) ->
    Id = maps:get(<<"id">>, Message, null),
    send_error(Id, -32600, <<"Invalid Request">>, invalid_request),
    State.

handle_request(Id, #{<<"method">> := <<"tools/call">>} = Message, State) ->
    start_tool(Id, maps:get(<<"params">>, Message, #{}), State);
handle_request(Id, Message, State) ->
    case request(maps:get(adapter, State), Message) of
        {ok, Result} ->
            send_result(Id, Result),
            case maps:get(<<"method">>, Message) of
                <<"initialize">> ->
                    Protocol = maps:get(<<"protocolVersion">>, Result),
                    State#{protocol => Protocol};
                _ -> State
            end;
        {error, Code, Text, Data} ->
            send_error(Id, Code, Text, Data),
            State
    end.

handle_notification(<<"notifications/initialized">>, _Params, State) -> State;
handle_notification(<<"notifications/cancelled">>, Params, State) when is_map(Params) ->
    cancel_request(maps:get(<<"requestId">>, Params, undefined), State);
handle_notification(_Method, _Params, State) -> State.

start_tool(Id, Params, State) when is_map(Params) ->
    Name = maps:get(<<"name">>, Params, undefined),
    Args = maps:get(<<"arguments">>, Params, #{}),
    Pending = maps:get(pending, State),
    case {is_binary(Name), is_map(Args), maps:is_key(Id, Pending)} of
        {true, true, false} ->
            Parent = self(),
            Adapter = maps:get(adapter, State),
            {Pid, Monitor} = spawn_monitor(fun() ->
                Parent ! {tool_result, self(), Id, Adapter:call(Name, Args)}
            end),
            Job = #{pid => Pid, monitor => Monitor},
            State#{pending => Pending#{Id => Job},
                   monitors => (maps:get(monitors, State))#{Monitor => Id}};
        {_, _, true} ->
            send_error(Id, -32600, <<"Duplicate request id">>, Id),
            State;
        _ ->
            send_error(Id, -32602, <<"Invalid params">>, name_and_arguments_required),
            State
    end;
start_tool(Id, _Params, State) ->
    send_error(Id, -32602, <<"Invalid params">>, object_required),
    State.

complete_tool(Pid, Id, Result, State) ->
    case maps:get(Id, maps:get(pending, State), undefined) of
        #{pid := Pid, monitor := Monitor} ->
            erlang:demonitor(Monitor, [flush]),
            send_result(Id, tool_result(Result, maps:get(protocol, State))),
            remove_pending(Id, Monitor, State);
        _ -> State
    end.

tool_down(Monitor, Reason, State) ->
    case maps:get(Monitor, maps:get(monitors, State), undefined) of
        undefined -> State;
        Id ->
            send_result(Id, tool_result({error, {tool_worker_down, Reason}},
                                        maps:get(protocol, State))),
            remove_pending(Id, Monitor, State)
    end.

cancel_request(undefined, State) -> State;
cancel_request(Id, State) ->
    case maps:get(Id, maps:get(pending, State), undefined) of
        undefined -> State;
        #{pid := Pid, monitor := Monitor} ->
            exit(Pid, cancelled),
            erlang:demonitor(Monitor, [flush]),
            remove_pending(Id, Monitor, State)
    end.

remove_pending(Id, Monitor, State) ->
    State#{pending => maps:remove(Id, maps:get(pending, State)),
           monitors => maps:remove(Monitor, maps:get(monitors, State))}.

cancel_all(State) ->
    lists:foreach(fun({_Id, #{pid := Pid}}) -> exit(Pid, shutdown) end,
                  maps:to_list(maps:get(pending, State))),
    ok.

tool_result({ok, Value}, Protocol) ->
    structured_result(#{<<"result">> => retinue_json:normalize(Value)}, false, Protocol);
tool_result({error, Reason}, Protocol) ->
    structured_result(#{<<"error">> => retinue_json:normalize(Reason)}, true, Protocol).

structured_result(Structured, IsError, <<"2024-11-05">>) ->
    #{<<"content">> => [#{<<"type">> => <<"text">>,
                           <<"text">> => retinue_json:encode(Structured)}],
      <<"isError">> => IsError};
structured_result(Structured, IsError, _Protocol) ->
    Text = case IsError of
        true -> retinue_json:encode(Structured);
        false -> <<"Structured result attached.">>
    end,
    #{<<"content">> => [#{<<"type">> => <<"text">>, <<"text">> => Text}],
      <<"structuredContent">> => Structured,
      <<"isError">> => IsError}.

send_result(Id, Result) ->
    send_json(#{<<"jsonrpc">> => <<"2.0">>, <<"id">> => Id, <<"result">> => Result}).

send_error(Id, Code, Message, Data) ->
    Error = #{<<"code">> => Code, <<"message">> => Message,
              <<"data">> => retinue_json:normalize(Data)},
    send_json(#{<<"jsonrpc">> => <<"2.0">>, <<"id">> => Id, <<"error">> => Error}).

send_json(Message) ->
    io:put_chars(standard_io, [retinue_json:encode(Message), $\n]).

read_loop(Parent) ->
    case io:get_line(standard_io, "") of
        eof -> Parent ! mcp_eof;
        {error, Reason} -> Parent ! {mcp_reader_error, Reason};
        Line ->
            Parent ! {mcp_line, Line},
            read_loop(Parent)
    end.

negotiate_protocol(Version) ->
    Supported = [?LATEST_PROTOCOL, <<"2025-06-18">>, <<"2025-03-26">>, <<"2024-11-05">>],
    case lists:member(Version, Supported) of true -> Version; false -> ?LATEST_PROTOCOL end.

app_version() ->
    unicode:characters_to_binary(retinue_build:version()).
