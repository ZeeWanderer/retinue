-module(wfcli_update_eunit).
-include_lib("eunit/include/eunit.hrl").

failure_diagnosis_test_() ->
    Url = "https://example.invalid/data.json",
    Cases = [
        {{http_status, Url, 404}, <<"publication may be delayed">>},
        {{http_status, Url, 410}, <<"removed">>},
        {{http_status, Url, 403}, <<"access denied">>},
        {{http_status, Url, 429}, <<"rate limited">>},
        {{http_status, Url, 503}, <<"upstream service failure">>},
        {{http_failed, Url, timeout}, <<"timed out">>},
        {{http_failed, Url, {failed_connect, [{to_address, {"example.invalid", 443}},
                                             {inet, [inet], nxdomain}]}}, <<"DNS lookup failed">>},
        {{http_failed, Url, {tls_alert, {unknown_ca, "detail"}}}, <<"TLS">>},
        {{file_integrity_mismatch, Url}, <<"checksum mismatch">>},
        {{file_size_mismatch, Url, 100, 50}, <<"expected 100 bytes, got 50">>},
        {{download_size_limit, Url}, <<"safety limit">>},
        {{invalid_data, Url, invalid_json}, <<"not valid JSON">>},
        {{invalid_data, Url, {incomplete_catalog, [<<"Mods">>]}}, <<"layout change">>},
        {{invalid_data, Url, {invalid_catalog_item, <<"/bad">>}}, <<"schema change">>},
        {{invalid_data, Url, {unresolved_component, <<"/part">>}}, <<"/part">>},
        {{invalid_data, Url, invalid_file_manifest}, <<"manifest">>}
    ],
    [?_test(begin
        Message = unicode:characters_to_binary(wfcli_update_cli:format_error(Reason)),
        ?assertNotEqual(nomatch, binary:match(Message, Text)),
        ?assertNotEqual(nomatch, binary:match(Message, list_to_binary(Url)))
    end) || {Reason, Text} <- Cases].

all_mirror_errors_are_reported_test() ->
    Error = {wfcd_sources_failed, <<"2.0.0">>,
             [{unpkg, {http_status, "https://unpkg.com/file", 404}},
              {jsdelivr, {invalid_data, "https://cdn.jsdelivr.net/file", invalid_catalog_root}}]},
    Message = unicode:characters_to_binary(wfcli_update_cli:format_error(Error)),
    lists:foreach(fun(Text) -> ?assertNotEqual(nomatch, binary:match(Message, Text)) end,
                  [<<"2.0.0">>, <<"existing caches kept">>, <<"unpkg">>, <<"HTTP 404">>,
                   <<"jsdelivr">>, <<"data layout">>]).

discovery_failure_reports_retained_cache_test() ->
    Message = unicode:characters_to_binary(wfcli_update_cli:format_error(
        {wfcd_version_failed, {http_failed, "https://registry.npmjs.org", timeout}})),
    ?assertNotEqual(nomatch, binary:match(Message, <<"latest WFCD version">>)),
    ?assertNotEqual(nomatch, binary:match(Message, <<"existing caches kept">>)).

storage_failure_is_not_mislabeled_as_network_or_layout_test_() ->
    [?_test(begin
        Message = unicode:characters_to_binary(wfcli_update_cli:format_error({Action, "/cache/file", Reason})),
        ?assertNotEqual(nomatch, binary:match(Message, <<"could not save cache">>)),
        ?assertNotEqual(nomatch, binary:match(Message, <<"/cache/file">>)),
        ?assertEqual(nomatch, binary:match(Message, <<"layout">>))
    end) || {Action, Reason} <- [{write_failed, enospc}, {mkdir_failed, eacces}]].
