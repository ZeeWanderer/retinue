-module(wfcli_notification_cli).

-export([command/0]).

command() ->
    #{help => "configure fissure notifications", handler => fun(_) -> show() end,
      commands => #{
        "status" => #{help => "show notification policy", handler => fun(_) -> show() end},
        "off" => #{help => "disable notifications", handler => fun(_) -> set_mode("off") end},
        "on" => #{help => "notify while a GUI is connected", handler => fun(_) -> set_mode("on") end},
        "persistent" => #{help => "notify while daemon is running",
                          handler => fun(_) -> set_mode("persistent") end}}}.

show() ->
    case retinue_client:call(warframe, notification_settings) of
        {ok, Settings} when is_map(Settings) -> print(Settings);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

set_mode(Name) ->
    Mode = case Name of "on" -> <<"session">>; _ -> list_to_binary(Name) end,
    Patch = #{<<"fissures">> => #{<<"mode">> => Mode}},
    case retinue_client:call(warframe, {notification_settings, Patch}) of
        {ok, Settings} when is_map(Settings) -> print(Settings);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

print(Settings) ->
    Fissures = maps:get(<<"fissures">>, Settings, #{}),
    Mode = maps:get(<<"mode">>, Fissures, <<"off">>),
    Display = case Mode of <<"session">> -> <<"on">>; _ -> Mode end,
    retinue_output:emit({json, Settings},
        fun() -> io:format("Fissure notifications: ~ts~n", [Display]) end).

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).
