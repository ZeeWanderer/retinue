%%%-------------------------------------------------------------------
%% Star Chart mastery metadata missing from the official PublicExport.
%%%-------------------------------------------------------------------
-module(wf_star_chart).

-include_lib("kernel/include/file.hrl").

-export([load/0, source/0, update/0]).
-ifdef(TEST).
-export([compact/1]).
-endif.

-define(CACHE_FILE, "StarChart.json").
-define(URL,
        "https://raw.githubusercontent.com/calamity-inc/warframe-public-export-plus/"
        "senpai/ExportRegions.json").
-define(CACHE_KEY, {?MODULE, cache}).

-doc "Load compact node mastery metadata, cached by file signature.".
-spec load() -> {ok, map(), map()} | {error, term()}.
load() ->
    Path = source(),
    case file:read_file_info(Path, [{time, posix}]) of
        {ok, #file_info{mtime = Modified, size = Size}} ->
            Signature = {Path, Modified, Size},
            case persistent_term:get(?CACHE_KEY, undefined) of
                #{signature := Signature, chart := Chart, meta := Meta} ->
                    {ok, Chart, Meta};
                _ -> load_file(Path, Signature)
            end;
        {error, Reason} -> {error, {star_chart_missing, Path, Reason}}
    end.

-doc "Return preferred managed Star Chart metadata path.".
-spec source() -> file:filename_all().
source() ->
    case application:get_env(retinue_daemon, star_chart_file) of
        {ok, Path} when is_list(Path); is_binary(Path) -> Path;
        _ -> default_source()
    end.

default_source() ->
    Paths = wf_worldstate:metadata_paths(?CACHE_FILE),
    case [Path || Path <- Paths, filelib:is_file(Path)] of
        [Path | _] -> Path;
        [] -> case Paths of
                  [Path | _] -> Path;
                  [] -> wf_paths:cache_file(?CACHE_FILE)
              end
    end.

-doc "Refresh compact Star Chart mastery metadata.".
-spec update() -> ok | {error, term()}.
update() ->
    application:ensure_all_started(inets),
    application:ensure_all_started(ssl),
    Headers = [{"user-agent", "retinue (+https://github.com/ZeeWanderer/retinue)"},
               {"accept", "application/json"}],
    case httpc:request(get, {?URL, Headers}, [{timeout, 30000}],
                       [{body_format, binary}]) of
        {ok, {{_, 200, _}, _ResponseHeaders, Body}} -> persist(Body);
        {ok, {{_, Code, _}, _ResponseHeaders, _Body}} ->
            {error, {star_chart_http_status, Code}};
        {error, Reason} -> {error, {star_chart_http_failed, Reason}}
    end.

load_file(Path, Signature) ->
    case file:read_file(Path) of
        {ok, Body} ->
            try jsone:decode(Body, [{object_format, map}]) of
                #{<<"nodes">> := Nodes, <<"junctions">> := Junctions} = Wrapper
                  when is_map(Nodes), is_list(Junctions) ->
                    Chart = #{nodes => Nodes,
                              junctions => maps:from_keys(Junctions, true)},
                    Meta = #{version => maps:get(<<"version">>, Wrapper, <<"unknown">>),
                             fetched_at => maps:get(<<"fetchedAt">>, Wrapper, 0)},
                    persistent_term:put(
                      ?CACHE_KEY,
                      #{signature => Signature, chart => Chart, meta => Meta}),
                    {ok, Chart, Meta};
                _ -> {error, {bad_star_chart, Path}}
            catch error:Reason -> {error, {bad_star_chart_json, Path, Reason}}
            end;
        {error, Reason} -> {error, {star_chart_read_failed, Path, Reason}}
    end.

persist(Body) ->
    try jsone:decode(Body, [{object_format, map}]) of
        Regions when is_map(Regions) ->
            Chart = compact(Regions),
            Wrapper = #{<<"source">> => list_to_binary(?URL),
                        <<"version">> => content_version(Body),
                        <<"fetchedAt">> => erlang:system_time(second),
                        <<"nodes">> => maps:get(nodes, Chart),
                        <<"junctions">> => maps:keys(maps:get(junctions, Chart))},
            case wf_worldstate:write_metadata_file(?CACHE_FILE, jsone:encode(Wrapper)) of
                ok -> persistent_term:erase(?CACHE_KEY), ok;
                Error -> Error
            end;
        _ -> {error, bad_star_chart_payload}
    catch error:Reason -> {error, {bad_star_chart_json, Reason}}
    end.

compact(Regions) ->
    maps:fold(
      fun(Id, Region, Acc) when is_binary(Id), is_map(Region) ->
              case is_junction(Id) of
                  true ->
                      case binary:match(Id, <<"To">>) of
                          nomatch -> Acc;
                          _ -> Acc#{junctions := (maps:get(junctions, Acc))#{Id => true}}
                      end;
                  false ->
                      case maps:get(<<"masteryExp">>, Region, 0) of
                          Xp when is_integer(Xp), Xp > 0 ->
                              Acc#{nodes := (maps:get(nodes, Acc))#{Id => Xp}};
                          _ -> Acc
                      end
              end;
         (_Id, _Region, Acc) -> Acc
      end,
      #{nodes => #{}, junctions => #{}},
      Regions).

is_junction(Id) ->
    Suffix = <<"Junction">>,
    byte_size(Id) >= byte_size(Suffix) andalso
        binary:part(Id, byte_size(Id) - byte_size(Suffix), byte_size(Suffix)) =:= Suffix.

content_version(Body) ->
    application:ensure_all_started(crypto),
    binary:encode_hex(crypto:hash(sha256, Body), lowercase).
