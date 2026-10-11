-module(wf_local_handler).
-export([init/1, request/2, event/2]).
-define(MAX_MARKET_RESOLVE_LABELS, 20).
-define(MAX_MARKET_RESOLVE_LIMIT, 5).
-import(retinue_connection, [send_json/2, send_error/3, send_ok/4, request_id/1, start_local_request/4, feature_enabled/2, daemon_identity/0]).

init(State) ->
    ensure_build_group_subscription(State#{subscriptions => #{}, refs => #{}, market_refs => #{},
              market_account_refs => #{},
              build_refs => #{},
              build_group_ref => undefined,
              market_presence_ref => undefined,
              asset_ref => undefined,
              activity_refs => #{}}).

request(#{<<"op">> := <<"companion_diagnostics">>, <<"request_id">> := Id,
                 <<"data">> := Data}, State)
  when is_binary(Id), byte_size(Id) =:= 32, is_map(Data) ->
    case maps:get(client, State, undefined) =:= <<"wfcompanion">> andalso
         feature_enabled(<<"companion.diagnostics">>, State) andalso
         erlang:external_size(Data) =< 65536 of
        true ->
            maps:get(parent, State) ! {diagnostic_reply, self(), Id, Data},
            {ok, State};
        false -> {stop, invalid_companion_diagnostic, State}
    end;
request(#{<<"op">> := <<"diagnostics_report">>,
                 <<"issues">> := Issues} = Request, State)
  when is_list(Issues) ->
    Id = request_id(Request),
    case feature_enabled(<<"diagnostics.report">>, State) of
        true ->
            Client = maps:get(client, State, <<"unknown">>),
            Scope = <<"client:", Client/binary>>,
            ok = wf_resolution_issues:reconcile(Scope, Issues),
            send_json(maps:get(socket, State), #{<<"id">> => Id, <<"ok">> => true});
        false ->
            send_error(maps:get(socket, State), Id, feature_not_negotiated)
    end,
    {ok, State};
request(#{<<"op">> := <<"diagnostics_report">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_diagnostics_report),
    {ok, State};
request(#{<<"op">> := <<"get">>, <<"dataset">> := Dataset} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, Dataset, fun() -> dataset_snapshot(Dataset) end, State)};
request(#{<<"op">> := <<"foundry_view">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"foundry">>,
                             fun wf_player_views:foundry/0, State)};
request(#{<<"op">> := <<"inventory_view">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"inventory">>,
                             fun wf_player_views:inventory/0, State)};
request(#{<<"op">> := <<"mastery_view">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"mastery">>,
                             fun wf_player_views:mastery/0, State)};
request(#{<<"op">> := <<"build_equipment">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"build_equipment">>,
                             fun wf_build_equipment:snapshot/0, State)};
request(#{<<"op">> := <<"build_stats">>} = Request, State) ->
    submit_build(request_id(Request), <<"build_stats">>,
                 #{source => build_stats, options => maps:get(<<"options">>, Request, #{})},
                 State);
request(#{<<"op">> := <<"build_search">>} = Request, State) ->
    BuildRequest = #{source => overframe, action => search,
                     query => maps:get(<<"query">>, Request, <<>>),
                     class => maps:get(<<"class">>, Request, <<"all">>),
                     limit => maps:get(<<"limit">>, Request, 40)},
    submit_build(request_id(Request), <<"build_items">>, BuildRequest, State);
request(#{<<"op">> := <<"build_list">>,
                 <<"item">> := Item} = Request, State) when is_binary(Item) ->
    BuildRequest0 = #{source => overframe, action => list, item => Item,
                      query => maps:get(<<"query">>, Request, <<>>),
                      scope => maps:get(<<"scope">>, Request, <<"public">>),
                      ordering => maps:get(<<"ordering">>, Request, <<"score">>),
                      limit => maps:get(<<"limit">>, Request, 30),
                      offset => maps:get(<<"offset">>, Request, 0)},
    BuildRequest = maybe_refresh(Request, BuildRequest0),
    submit_build(request_id(Request), <<"builds">>, BuildRequest, State);
request(#{<<"op">> := <<"build_list">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_build_item),
    {ok, State};
request(#{<<"op">> := <<"build_detail">>,
                 <<"build_id">> := BuildId} = Request, State)
  when is_integer(BuildId), BuildId > 0 ->
    BuildRequest = maybe_refresh(
                     Request, #{source => overframe, action => detail, id => BuildId}),
    submit_build(request_id(Request), <<"build_revision">>, BuildRequest, State);
request(#{<<"op">> := <<"build_detail">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_build_id),
    {ok, State};
request(#{<<"op">> := <<"build_group_list">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"build_groups">>,
                             fun wf_build_service:groups/0, State)};
request(#{<<"op">> := <<"build_group_get">>,
                 <<"group_id">> := GroupId} = Request, State)
  when is_binary(GroupId) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"build_group">>,
                             fun() -> wf_build_service:group(GroupId) end,
                             State)};
request(#{<<"op">> := <<"build_group_create">>,
                 <<"group">> := Input} = Request, State) when is_map(Input) ->
    Id = request_id(Request),
    {ok, start_local_request(
           Id, <<"build_group">>,
           fun() -> with_build_equipment(
                      fun(Equipment) ->
                          wf_build_service:create_group(Input, Equipment)
                      end)
           end, State)};
request(#{<<"op">> := <<"build_group_update">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision,
                 <<"patch">> := Patch} = Request, State)
  when is_binary(GroupId), is_integer(Revision), is_map(Patch) ->
    Id = request_id(Request),
    {ok, start_local_request(
           Id, <<"build_group">>,
           fun() -> with_build_equipment(
                      fun(Equipment) ->
                          wf_build_service:update_group(GroupId, Revision,
                                                           Patch, Equipment)
                      end)
           end, State)};
request(#{<<"op">> := <<"build_group_delete">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision} = Request,
               State) when is_binary(GroupId), is_integer(Revision) ->
    Id = request_id(Request),
    Work = fun() ->
        case wf_build_service:delete_group(GroupId, Revision) of
            ok -> {ok, #{<<"id">> => GroupId, <<"deleted">> => true}};
            {error, _Reason} = Error -> Error
        end
    end,
    {ok, start_local_request(Id, <<"build_group">>, Work, State)};
request(#{<<"op">> := <<"build_group_add_source">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision,
                 <<"source">> := Source, <<"external_id">> := ExternalId} = Request,
               State)
  when is_binary(GroupId), is_integer(Revision), is_binary(Source) ->
    Id = request_id(Request),
    Fingerprint = case maps:get(<<"fingerprint">>, Request, null) of
        Value when is_binary(Value) -> Value;
        _ -> latest
    end,
    Work = fun() -> wf_build_service:add_source_member(
                       GroupId, Revision, Source, ExternalId, Fingerprint) end,
    {ok, start_local_request(Id, <<"build_group">>, Work, State)};
request(#{<<"op">> := <<"build_group_add_config">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision,
                 <<"instance_id">> := InstanceId,
                 <<"config_index">> := ConfigIndex} = Request, State)
  when is_binary(GroupId), is_integer(Revision), is_binary(InstanceId),
       is_integer(ConfigIndex), ConfigIndex >= 0 ->
    Id = request_id(Request),
    {ok, start_local_request(
           Id, <<"build_group">>,
           fun() -> with_build_equipment(
                      fun(Equipment) ->
                          wf_build_service:add_config_member(
                            GroupId, Revision, InstanceId, ConfigIndex, Equipment)
                      end)
           end, State)};
request(#{<<"op">> := <<"build_group_remove_member">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision,
                 <<"member_id">> := MemberId} = Request, State)
  when is_binary(GroupId), is_integer(Revision), is_binary(MemberId) ->
    Id = request_id(Request),
    Work = fun() -> wf_build_service:remove_member(
                       GroupId, Revision, MemberId) end,
    {ok, start_local_request(Id, <<"build_group">>, Work, State)};
request(#{<<"op">> := <<"build_group_plan">>,
                 <<"group_id">> := GroupId, <<"revision">> := Revision} = Request,
               State) when is_binary(GroupId), is_integer(Revision) ->
    Id = request_id(Request),
    Work = fun() ->
        case wf_build_service:plan_group(self(), GroupId, Revision) of
            {ok, Ref} -> receive {retinue_build, Ref, Reply} -> Reply end;
            Error -> Error
        end
    end,
    {ok, start_local_request(Id, <<"build_plan">>, Work, State)};
request(#{<<"op">> := Op} = Request, State)
  when Op =:= <<"build_group_get">>; Op =:= <<"build_group_create">>;
       Op =:= <<"build_group_update">>; Op =:= <<"build_group_delete">>;
       Op =:= <<"build_group_add_source">>; Op =:= <<"build_group_add_config">>;
       Op =:= <<"build_group_remove_member">>; Op =:= <<"build_group_plan">> ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_build_group_request),
    {ok, State};
request(#{<<"op">> := <<"activity_view">>} = Request, State) ->
    Id = request_id(Request),
    WorldstateRequest = #{source => worldstate, mode => list,
                          opts => #{ttl => 60, resolve_items => true}},
    case wf_worldstate_service:submit(self(), WorldstateRequest) of
        {ok, Ref} ->
            Refs = maps:get(activity_refs, State),
            {ok, State#{activity_refs => Refs#{Ref => Id}}};
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end;
request(#{<<"op">> := <<"notification_settings">>} = Request, State) ->
    send_ok(maps:get(socket, State), request_id(Request), <<"notifications">>,
            wf_notification_service:settings()),
    {ok, State};
request(#{<<"op">> := <<"notification_settings_set">>,
                 <<"fissures">> := Fissures} = Request, State) when is_map(Fissures) ->
    case wf_notification_service:update(#{<<"fissures">> => Fissures}) of
        {ok, Settings} ->
            send_ok(maps:get(socket, State), request_id(Request),
                    <<"notifications">>, Settings);
        {error, Reason} ->
            send_error(maps:get(socket, State), request_id(Request), Reason)
    end,
    {ok, State};
request(#{<<"op">> := <<"notification_settings_set">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_notification_settings),
    {ok, State};
request(#{<<"op">> := <<"subscribe">>, <<"dataset">> := <<"player">>} = Request,
               State) ->
    subscribe_player(request_id(Request), Request, State);
request(#{<<"op">> := <<"unsubscribe">>, <<"subscription">> := Id} = Request,
               State) when is_integer(Id) ->
    State1 = unsubscribe_player(Id, State),
    send_json(maps:get(socket, State),
              #{<<"id">> => request_id(Request), <<"ok">> => true}),
    {ok, State1};
request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"player">>,
                 <<"source">> := <<"inventory">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), inventory_is_daemon_owned),
    {ok, State};
request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"player">>,
                 <<"source">> := Source, <<"data">> := Data} = Request, State)
  when is_binary(Source), is_map(Data) ->
    case wf_player_service:publish(Source, Data, metadata) of
        {ok, Snapshot} ->
            send_json(maps:get(socket, State),
                      #{<<"id">> => request_id(Request), <<"ok">> => true,
                        <<"revision">> => maps:get(revision, Snapshot)});
        {error, Reason} -> send_error(maps:get(socket, State), request_id(Request), Reason)
    end,
    {ok, State};
request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"game_metadata">>,
                 <<"data">> := Data} = Request, State) when is_map(Data) ->
    case wf_game_metadata_service:publish(Data) of
        {ok, Snapshot} ->
            send_json(maps:get(socket, State),
                      #{<<"id">> => request_id(Request), <<"ok">> => true,
                        <<"revision">> => maps:get(revision, Snapshot)});
        {error, Reason} -> send_error(maps:get(socket, State), request_id(Request), Reason)
    end,
    {ok, State};
request(#{<<"op">> := <<"market_resolve">>} = Request, State) ->
    Id = request_id(Request),
    Labels = maps:get(<<"labels">>, Request, undefined),
    Limit = maps:get(<<"limit">>, Request, undefined),
    case {valid_market_labels(Labels), valid_market_limit(Limit)} of
        {false, _} ->
            send_error(maps:get(socket, State), Id, invalid_market_labels),
            {ok, State};
        {true, false} ->
            send_error(maps:get(socket, State), Id, invalid_market_limit),
            {ok, State};
        {true, true} ->
            MarketRequest = #{source => market, action => resolve_labels,
                              labels => Labels, limit => Limit},
            case wf_market_service:submit(self(), MarketRequest) of
                {ok, Ref} ->
                    Refs = maps:get(market_refs, State),
                    {ok, State#{market_refs => Refs#{Ref => Id}}};
                {error, Reason} ->
                    send_error(maps:get(socket, State), Id, Reason),
                    {ok, State}
            end
    end;
request(#{<<"op">> := <<"market_quote">>, <<"items">> := Items} = Request, State)
  when is_list(Items), length(Items) =< 100 ->
    Id = request_id(Request),
    Ttl = maps:get(<<"ttl">>, Request, 60),
    case is_integer(Ttl) andalso Ttl >= 60 of
        true ->
            MarketRequest = #{source => market, action => quote_items, items => Items,
                              ttl => Ttl,
                              cache_only =>
                                  maps:get(<<"cache_only">>, Request, false) =:= true,
                              refresh => maps:get(<<"refresh">>, Request, false) =:= true},
            case wf_market_service:submit(self(), MarketRequest) of
                {ok, Ref} ->
                    Refs = maps:get(market_refs, State),
                    {ok, State#{market_refs => Refs#{Ref => Id}}};
                {error, Reason} ->
                    send_error(maps:get(socket, State), Id, Reason),
                    {ok, State}
            end;
        false ->
            send_error(maps:get(socket, State), Id, invalid_market_ttl),
            {ok, State}
    end;
request(#{<<"op">> := <<"market_quote">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_market_items),
    {ok, State};
request(#{<<"op">> := <<"market_quote_variant">>, <<"item">> := Item,
                 <<"filters">> := Filters} = Request, State)
  when is_binary(Item), byte_size(Item) > 0, is_map(Filters) ->
    case market_variant_filters(Filters) of
        {ok, Normalized} ->
            submit_market(request_id(Request),
                          #{source => market, action => quote_variant,
                            item => Item, filters => Normalized, ttl => 60,
                            refresh => maps:get(<<"refresh">>, Request, false) =:= true},
                          State);
        error ->
            send_error(maps:get(socket, State), request_id(Request),
                       invalid_market_quote_filters),
            {ok, State}
    end;
request(#{<<"op">> := <<"market_quote_variant">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_market_quote_filters),
    {ok, State};
request(#{<<"op">> := <<"market_describe">>,
                 <<"items">> := Items} = Request, State)
  when is_list(Items), length(Items) =< 100 ->
    case valid_market_items(Items) of
        true -> submit_market(
                  request_id(Request),
                  #{source => market, action => describe_items, items => Items}, State);
        false ->
            send_error(maps:get(socket, State), request_id(Request), invalid_market_items),
            {ok, State}
    end;
request(#{<<"op">> := <<"market_describe">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_market_items),
    {ok, State};
request(#{<<"op">> := <<"market_account">>} = Request, State) ->
    submit_market_account(request_id(Request), #{action => snapshot},
                          ensure_market_presence_subscription(State));
request(#{<<"op">> := <<"market_presence">>} = Request, State) ->
    State1 = ensure_market_presence_subscription(State),
    send_ok(maps:get(socket, State1), request_id(Request), <<"market_presence">>,
            wf_market_presence_service:snapshot()),
    {ok, State1};
request(#{<<"op">> := <<"market_presence_set">>,
                 <<"mode">> := Mode} = Request, State) when is_binary(Mode) ->
    State1 = ensure_market_presence_subscription(State),
    case wf_market_presence_service:set_mode(Mode) of
        {ok, Presence} ->
            send_ok(maps:get(socket, State1), request_id(Request),
                    <<"market_presence">>, Presence);
        {error, Reason} -> send_error(maps:get(socket, State1), request_id(Request), Reason)
    end,
    {ok, State1};
request(#{<<"op">> := <<"market_presence_set">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_market_presence_mode),
    {ok, State};
request(#{<<"op">> := <<"market_login">>, <<"email">> := Email,
                 <<"password">> := Password} = Request, State)
  when is_binary(Email), byte_size(Email) > 0, byte_size(Email) =< 256,
       is_binary(Password), byte_size(Password) > 0, byte_size(Password) =< 256 ->
    submit_market_account(request_id(Request),
                          #{action => login, email => Email, password => Password}, State);
request(#{<<"op">> := <<"market_login">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_market_credentials),
    {ok, State};
request(#{<<"op">> := <<"market_logout">>} = Request, State) ->
    submit_market_account(request_id(Request), #{action => logout}, State);
request(#{<<"op">> := <<"overframe_account">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"overframe_account">>,
                             fun wf_overframe_account:snapshot/0, State)};
request(#{<<"op">> := <<"overframe_session_import">>,
                 <<"cookies">> := Cookies} = Request, State) when is_list(Cookies) ->
    Id = request_id(Request),
    Work = fun() -> wf_overframe_account:store_session(Cookies) end,
    {ok, start_local_request(Id, <<"overframe_account">>, Work, State)};
request(#{<<"op">> := <<"overframe_session_import">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request),
               invalid_overframe_session),
    {ok, State};
request(#{<<"op">> := <<"overframe_logout">>} = Request, State) ->
    Id = request_id(Request),
    {ok, start_local_request(Id, <<"overframe_account">>,
                             fun wf_overframe_account:logout/0, State)};
request(#{<<"op">> := <<"market_order_create">>,
                 <<"order">> := Order} = Request, State) when is_map(Order) ->
    case valid_market_order(Order) of
        true -> submit_market_account(request_id(Request),
                                      #{action => create_order, order => Order}, State);
        false ->
            send_error(maps:get(socket, State), request_id(Request), invalid_market_order),
            {ok, State}
    end;
request(#{<<"op">> := <<"market_order_update">>, <<"order_id">> := OrderId,
                 <<"patch">> := Patch} = Request, State)
  when is_binary(OrderId), byte_size(OrderId) > 0, is_map(Patch) ->
    case valid_market_order_patch(Patch) of
        true -> submit_market_account(request_id(Request),
                                      #{action => update_order, id => OrderId,
                                        patch => Patch}, State);
        false ->
            send_error(maps:get(socket, State), request_id(Request),
                       invalid_market_order_patch),
            {ok, State}
    end;
request(#{<<"op">> := <<"market_order_delete">>,
                 <<"order_id">> := OrderId} = Request, State)
  when is_binary(OrderId), byte_size(OrderId) > 0 ->
    submit_market_account(request_id(Request),
                          #{action => delete_order, id => OrderId}, State);
request(#{<<"op">> := <<"market_order_close">>, <<"order_id">> := OrderId,
                 <<"quantity">> := Quantity} = Request, State)
  when is_binary(OrderId), byte_size(OrderId) > 0,
       is_integer(Quantity), Quantity >= 1, Quantity =< 9999 ->
    submit_market_account(request_id(Request),
                          #{action => close_order, id => OrderId,
                            quantity => Quantity}, State);
request(#{<<"op">> := <<"market_orders_visibility">>,
                 <<"visible">> := Visible} = Request, State) when is_boolean(Visible) ->
    Type = maps:get(<<"type">>, Request, undefined),
    case Type =:= undefined orelse Type =:= <<"buy">> orelse Type =:= <<"sell">> of
        true -> submit_market_account(request_id(Request),
                                      #{action => set_visibility, visible => Visible,
                                        type => Type}, State);
        false ->
            send_error(maps:get(socket, State), request_id(Request),
                       invalid_market_order_type),
            {ok, State}
    end;
request(#{<<"op">> := MarketOperation} = Request, State)
  when MarketOperation =:= <<"market_order_create">>;
       MarketOperation =:= <<"market_order_update">>;
       MarketOperation =:= <<"market_order_delete">>;
       MarketOperation =:= <<"market_order_close">>;
       MarketOperation =:= <<"market_orders_visibility">> ->
    send_error(maps:get(socket, State), request_id(Request), invalid_market_order_request),
    {ok, State};
request(#{<<"op">> := <<"relic_context">>,
                 <<"items">> := Items} = Request, State)
  when is_list(Items), length(Items) =< 8 ->
    Id = request_id(Request),
    MarketRequest = #{source => market, action => relic_context, items => Items,
                      ttl => 60, refresh => false},
    case wf_market_service:submit(self(), MarketRequest) of
        {ok, Ref} ->
            Refs = maps:get(market_refs, State),
            {ok, State#{market_refs => Refs#{Ref => Id}}};
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end;
request(#{<<"op">> := <<"relic_context">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_relic_items),
    {ok, State};
request(#{<<"op">> := <<"relic_recommendations">>,
                 <<"era">> := Era} = Request, State) when is_binary(Era) ->
    Id = request_id(Request),
    case relic_limit(Request, 32) of
        {ok, Limit} ->
            MarketRequest = #{source => market, action => relic_recommendations,
                              era => Era, view => recommendations, limit => Limit,
                              only_owned => true,
                              fetch_prices =>
                                  maps:get(<<"fetch_prices">>, Request, false) =:= true},
            case wf_market_service:submit(self(), MarketRequest) of
                {ok, Ref} ->
                    Refs = maps:get(market_refs, State),
                    {ok, State#{market_refs => Refs#{Ref => Id}}};
                {error, Reason} ->
                    send_error(maps:get(socket, State), Id, Reason),
                    {ok, State}
            end;
        error ->
            send_error(maps:get(socket, State), Id, invalid_relic_limit),
            {ok, State}
    end;
request(#{<<"op">> := <<"relic_recommendations">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_relic_era),
    {ok, State};
request(#{<<"op">> := <<"relic_planner">>,
                 <<"era">> := Era} = Request, State) when is_binary(Era) ->
    Id = request_id(Request),
    case relic_limit(Request, all) of
        {ok, Limit} ->
            MarketRequest = #{source => market, action => relic_recommendations,
                              era => Era, view => planner, limit => Limit,
                              only_owned =>
                                  maps:get(<<"only_owned">>, Request, true) =:= true,
                              fetch_prices =>
                                  maps:get(<<"fetch_prices">>, Request, false) =:= true},
            case wf_market_service:submit(self(), MarketRequest) of
                {ok, Ref} ->
                    Refs = maps:get(market_refs, State),
                    {ok, State#{market_refs => Refs#{Ref => Id}}};
                {error, Reason} ->
                    send_error(maps:get(socket, State), Id, Reason),
                    {ok, State}
            end;
        error ->
            send_error(maps:get(socket, State), Id, invalid_relic_limit),
            {ok, State}
    end;
request(#{<<"op">> := <<"relic_planner">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_relic_era),
    {ok, State};
request(#{<<"op">> := <<"asset_resolve">>,
                 <<"assets">> := Assets} = Request, State)
  when is_list(Assets), length(Assets) =< 64 ->
    Id = request_id(Request),
    Work = fun() ->
        case wf_asset_service:resolve(Assets) of
            {ok, Results} -> {ok, #{<<"assets">> => Results}};
            {error, _Reason} = Error -> Error
        end
    end,
    State1 = ensure_asset_subscription(State),
    {ok, start_local_request(Id, <<"assets">>, Work, State1)};
request(#{<<"op">> := <<"asset_resolve">>} = Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), invalid_assets),
    {ok, State};
request(#{<<"op">> := <<"asset_cache_status">>} = Request, State) ->
    Id = request_id(Request),
    Work = fun() ->
        {ok, asset_cache_data(wf_asset_service:status())}
    end,
    {ok, start_local_request(Id, <<"asset_cache">>, Work, State)};
request(#{<<"op">> := <<"asset_cache_clear">>} = Request, State) ->
    Id = request_id(Request),
    Work = fun() ->
        case wf_asset_service:clear() of
            {ok, Status} -> {ok, asset_cache_data(Status)};
            {error, _Reason} = Error -> Error
        end
    end,
    {ok, start_local_request(Id, <<"asset_cache">>, Work, State)};
request(Request, State) ->
    send_error(maps:get(socket, State), request_id(Request), unsupported_request),
    {ok, State}.
event({wf_player, Ref, Source, Snapshot}, State) ->
            State1 = send_subscription_update(Ref, Source, Snapshot, State),
            State1;
event({retinue_daemon, Ref, Reply}, State) ->
            State1 = send_service_reply(Ref, Reply, State),
            State1;
event({wf_market_account, Ref, Reply}, State) ->
            State1 = send_market_account_reply(Ref, Reply, State),
            State1;
event({retinue_build, Ref, Reply}, State) ->
            State1 = send_build_reply(Ref, Reply, State),
            State1;
event({wf_build_group, Ref, Event, Group}, State) ->
            State1 = send_build_group_event(Ref, Event, Group, State),
            State1;
event({wf_market_presence, Ref, Snapshot}, State) ->
            State1 = send_market_presence_event(Ref, Snapshot, State),
            State1;
event({wf_asset, Ref, Descriptor}, State) ->
            State1 = send_asset_event(Ref, Descriptor, State),
            State1;
event(_Message, State) -> State.

valid_market_labels(Labels)
  when is_list(Labels), length(Labels) =< ?MAX_MARKET_RESOLVE_LABELS ->
    lists:all(fun(Label) -> is_binary(Label) andalso byte_size(Label) > 0 end, Labels);
valid_market_labels(_Labels) -> false.

valid_market_limit(Limit) ->
    is_integer(Limit) andalso Limit >= 1 andalso Limit =< ?MAX_MARKET_RESOLVE_LIMIT.

valid_market_items(Items) ->
    lists:all(fun(Item) -> is_binary(Item) andalso byte_size(Item) > 0 end, Items).

asset_cache_data(Status) ->
    #{<<"cache_root">> => retinue_text:to_binary(maps:get(cache_root, Status)),
      <<"entries">> => maps:get(entries, Status),
      <<"objects">> => maps:get(objects, Status),
      <<"bytes">> => maps:get(bytes, Status),
      <<"queued">> => maps:get(queued, Status),
      <<"pending">> => maps:get(pending, Status),
      <<"fetching">> => maps:get(fetching, Status),
      <<"waiting_calls">> => maps:get(waiting_calls, Status)}.

market_variant_filters(Filters) ->
    Allowed = #{<<"rank">> => {rank, 100}, <<"charges">> => {charges, 100000},
                <<"amberStars">> => {amber_stars, 100},
                <<"cyanStars">> => {cyan_stars, 100}},
    maps:fold(
      fun(_Key, _Value, error) -> error;
         (<<"subtype">>, Value, {ok, Acc}) when is_binary(Value),
                                                byte_size(Value) > 0,
                                                byte_size(Value) =< 128 ->
              {ok, Acc#{subtype => Value}};
         (Key, Value, {ok, Acc}) when is_integer(Value), Value >= 0 ->
              case maps:get(Key, Allowed, undefined) of
                  {Name, Max} when Value =< Max -> {ok, Acc#{Name => Value}};
                  _ -> error
              end;
         (_Key, _Value, {ok, _Acc}) -> error
      end,
      {ok, #{}}, Filters).

valid_market_order(Order) ->
    valid_nonempty_binary(maps:get(<<"itemId">>, Order, undefined)) andalso
    valid_market_order_type(maps:get(<<"type">>, Order, undefined)) andalso
    valid_integer(maps:get(<<"platinum">>, Order, undefined), 1, 900000) andalso
    valid_integer(maps:get(<<"quantity">>, Order, undefined), 1, 9999) andalso
    valid_optional_boolean(maps:get(<<"visible">>, Order, undefined)) andalso
    valid_order_options(Order).

valid_market_order_patch(Patch) when map_size(Patch) > 0 ->
    Allowed = [<<"platinum">>, <<"quantity">>, <<"visible">>, <<"perTrade">>,
               <<"rank">>, <<"charges">>, <<"subtype">>, <<"amberStars">>,
               <<"cyanStars">>],
    lists:all(fun(Key) -> lists:member(Key, Allowed) end, maps:keys(Patch)) andalso
    valid_optional_integer(maps:get(<<"platinum">>, Patch, undefined), 1, 900000) andalso
    valid_optional_integer(maps:get(<<"quantity">>, Patch, undefined), 1, 9999) andalso
    valid_optional_boolean(maps:get(<<"visible">>, Patch, undefined)) andalso
    valid_order_options(Patch);
valid_market_order_patch(_Patch) -> false.

valid_order_options(Order) ->
    valid_optional_integer(maps:get(<<"perTrade">>, Order, undefined), 1, 6) andalso
    valid_optional_integer(maps:get(<<"rank">>, Order, undefined), 0, 100) andalso
    valid_optional_integer(maps:get(<<"charges">>, Order, undefined), 0, 100000) andalso
    valid_optional_integer(maps:get(<<"amberStars">>, Order, undefined), 0, 100) andalso
    valid_optional_integer(maps:get(<<"cyanStars">>, Order, undefined), 0, 100) andalso
    valid_optional_binary(maps:get(<<"subtype">>, Order, undefined)).

valid_market_order_type(<<"buy">>) -> true;
valid_market_order_type(<<"sell">>) -> true;
valid_market_order_type(_Type) -> false.

valid_nonempty_binary(Value) -> is_binary(Value) andalso byte_size(Value) > 0.
valid_optional_binary(undefined) -> true;
valid_optional_binary(Value) -> valid_nonempty_binary(Value).
valid_optional_boolean(undefined) -> true;
valid_optional_boolean(Value) -> is_boolean(Value).
valid_integer(Value, Min, Max) -> is_integer(Value) andalso Value >= Min andalso Value =< Max.
valid_optional_integer(undefined, _Min, _Max) -> true;
valid_optional_integer(Value, Min, Max) -> valid_integer(Value, Min, Max).

relic_limit(Request, Default) ->
    case maps:get(<<"limit">>, Request, Default) of
        <<"all">> -> {ok, all};
        Limit when is_integer(Limit), Limit >= 1, Limit =< 1000 -> {ok, Limit};
        _ -> error
    end.

subscribe_player(Id, Request, State) when is_integer(Id) ->
    case subscription_options(Request) of
        {ok, Options} -> start_player_subscription(Id, Options, State);
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end;
subscribe_player(_Id, _Request, State) ->
    send_error(maps:get(socket, State), 0, invalid_request_id),
    {ok, State}.

start_player_subscription(Id, View, State) ->
    State1 = unsubscribe_player(Id, State),
    case wf_player_service:subscribe(self(), View) of
        {ok, Ref, Snapshot} ->
            send_ok(maps:get(socket, State1), Id, <<"player">>, json_snapshot(Snapshot)),
            Subscriptions = maps:get(subscriptions, State1),
            Refs = maps:get(refs, State1),
            {ok, State1#{subscriptions => Subscriptions#{Id => Ref},
                         refs => Refs#{Ref => Id}}};
        {error, Reason} ->
            send_error(maps:get(socket, State1), Id, Reason),
            {ok, State1}
    end.

unsubscribe_player(Id, State) ->
    case maps:take(Id, maps:get(subscriptions, State)) of
        error -> State;
        {Ref, Subscriptions} ->
            _ = wf_player_service:unsubscribe(Ref),
            Refs = maps:remove(Ref, maps:get(refs, State)),
            State#{subscriptions => Subscriptions, refs => Refs}
    end.

send_subscription_update(Ref, Source, Snapshot, State) ->
    case maps:get(Ref, maps:get(refs, State), undefined) of
        undefined -> State;
        Id when is_integer(Id) ->
            send_player_event(Id, Source, json_snapshot(Snapshot), State)
    end.

send_player_event(Id, Source, Snapshot, State) ->
    Event0 = #{<<"event">> => <<"dataset">>,
               <<"subscription">> => Id,
               <<"dataset">> => <<"player">>,
               <<"data">> => Snapshot},
    Event = case json_source(Source) of
        undefined -> Event0;
        JsonSource -> Event0#{<<"source">> => JsonSource}
    end,
    send_json(maps:get(socket, State), Event),
    State.

subscription_options(Request) ->
    IncludeData = maps:get(<<"include_data">>, Request, true),
    case {maps:get(<<"view">>, Request, <<"full">>), IncludeData} of
        {<<"full">>, true} -> {ok, full};
        {<<"full">>, false} -> {ok, metadata};
        {<<"hud">>, true} -> {ok, hud};
        _ -> {error, invalid_subscription_options}
    end.

json_source(clear) -> <<"clear">>;
json_source(Source) when is_binary(Source) -> Source;
json_source(_Source) -> undefined.

send_service_reply(Ref, Reply, State) ->
    case maps:take(Ref, maps:get(activity_refs, State, #{})) of
        error -> send_market_reply(Ref, Reply, State);
        {Id, ActivityRefs} ->
            Projected = case Reply of
                {ok, WorldstateData} -> wf_activity_view:project(WorldstateData);
                {error, _Reason} = Error -> Error
            end,
            case Projected of
                {ok, ActivityData} ->
                    send_ok(maps:get(socket, State), Id, <<"activity">>, ActivityData);
                {error, Reason} -> send_error(maps:get(socket, State), Id, Reason)
            end,
            State#{activity_refs => ActivityRefs}
    end.

send_market_reply(Ref, Reply, State) ->
    case maps:take(Ref, maps:get(market_refs, State, #{})) of
        error -> State;
        {Id, MarketRefs} ->
            case Reply of
                {ok, Data} -> send_ok(maps:get(socket, State), Id, <<"market">>, Data);
                {error, Reason} -> send_error(maps:get(socket, State), Id, Reason)
            end,
            State#{market_refs => MarketRefs}
    end.

send_market_account_reply(Ref, Reply, State) ->
    case maps:take(Ref, maps:get(market_account_refs, State, #{})) of
        error -> State;
        {Id, AccountRefs} ->
            case Reply of
                {ok, Data} ->
                    Account = Data#{<<"presence">> =>
                                        wf_market_presence_service:snapshot()},
                    send_ok(maps:get(socket, State), Id,
                            <<"market_account">>, Account);
                {error, Reason} -> send_error(maps:get(socket, State), Id, Reason)
            end,
            State#{market_account_refs => AccountRefs}
    end.

send_build_reply(Ref, Reply, State) ->
    case maps:take(Ref, maps:get(build_refs, State, #{})) of
        error -> State;
        {{Id, Dataset}, BuildRefs} ->
            case Reply of
                {ok, Data} -> send_ok(maps:get(socket, State), Id, Dataset, Data);
                {error, Reason} -> send_error(maps:get(socket, State), Id, Reason)
            end,
            State#{build_refs => BuildRefs}
    end.

send_build_group_event(Ref, Event, Group, State) ->
    case maps:get(build_group_ref, State, undefined) of
        Ref ->
            send_json(maps:get(socket, State),
                      #{<<"event">> => <<"build_group">>,
                        <<"action">> => atom_to_binary(Event),
                        <<"data">> => Group}),
            State;
        _ -> State
    end.

ensure_build_group_subscription(State = #{build_group_ref := Ref})
  when is_reference(Ref) -> State;
ensure_build_group_subscription(State) ->
    case wf_build_service:subscribe(self()) of
        {ok, Ref} -> State#{build_group_ref => Ref};
        _ -> State
    end.

send_market_presence_event(Ref, Snapshot, State) ->
    case maps:get(market_presence_ref, State, undefined) of
        Ref ->
            send_json(maps:get(socket, State),
                      #{<<"event">> => <<"market_presence">>,
                        <<"data">> => Snapshot}),
            State;
        _ -> State
    end.

ensure_asset_subscription(State) ->
    try wf_asset_service:subscribe(self()) of
        {ok, Ref} -> State#{asset_ref => Ref};
        _ -> State
    catch
        _:_ -> State
    end.

send_asset_event(Ref, Descriptor, State) ->
    case maps:get(asset_ref, State, undefined) of
        Ref ->
            send_json(maps:get(socket, State),
                      #{<<"event">> => <<"asset">>, <<"data">> => Descriptor}),
            State;
        _ -> State
    end.

ensure_market_presence_subscription(State = #{market_presence_ref := Ref})
  when is_reference(Ref) -> State;
ensure_market_presence_subscription(State) ->
    case wf_market_presence_service:subscribe(self()) of
        {ok, Ref, _Snapshot} -> State#{market_presence_ref => Ref};
        _ -> State
    end.

submit_market(Id, MarketRequest, State) ->
    case wf_market_service:submit(self(), MarketRequest) of
        {ok, Ref} ->
            Refs = maps:get(market_refs, State),
            {ok, State#{market_refs => Refs#{Ref => Id}}};
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end.

submit_market_account(Id, AccountRequest, State) ->
    case wf_market_account_service:submit(self(), AccountRequest) of
        {ok, Ref} ->
            Refs = maps:get(market_account_refs, State),
            {ok, State#{market_account_refs => Refs#{Ref => Id}}};
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end.

submit_build(Id, Dataset, BuildRequest, State) ->
    case wf_build_service:submit(self(), BuildRequest) of
        {ok, Ref} ->
            Refs = maps:get(build_refs, State, #{}),
            {ok, State#{build_refs => Refs#{Ref => {Id, Dataset}}}};
        {error, Reason} ->
            send_error(maps:get(socket, State), Id, Reason),
            {ok, State}
    end.

with_build_equipment(Fun) ->
    case wf_build_equipment:snapshot() of
        {ok, Equipment} -> Fun(Equipment);
        {error, _Reason} = Error -> Error
    end.

maybe_refresh(Request, BuildRequest) ->
    case maps:get(<<"refresh">>, Request, false) of
        true -> BuildRequest#{refresh => true};
        false -> BuildRequest
    end.

dataset_snapshot(<<"player">>) ->
    {ok, json_snapshot(wf_player_service:snapshot())};
dataset_snapshot(<<"game_metadata">>) ->
    {ok, json_snapshot(wf_game_metadata_service:snapshot())};
dataset_snapshot(<<"daemon">>) ->
    {ok, daemon_identity()};
dataset_snapshot(Dataset) ->
    {error, {unsupported_dataset, Dataset}}.

json_snapshot(Snapshot) ->
    Header = #{<<"revision">> => maps:get(revision, Snapshot),
               <<"updated_at">> => nullable(maps:get(updated_at, Snapshot))},
    case maps:find(data, Snapshot) of
        {ok, Data} -> Header#{<<"data">> => Data};
        error -> Header
    end.

nullable(undefined) -> null;
nullable(Value) -> Value.
