%%%-------------------------------------------------------------------
%% Terminal block layouts for Codex and WFCD entities.
%%%-------------------------------------------------------------------
-module(wfcli_knowledge_presentation).

-export([block_spec/1]).

-doc "Return terminal block layout for one knowledge result kind.".
-spec block_spec(codex | enemy | drop) -> map().
block_spec(codex) ->
    #{title => "Codex: {name}", fields => [{"Category", category}, {"Description", description},
                                            {"Secret", secret}, {"File", file}, {"Unique", uniqueName}]};
block_spec(enemy) ->
    #{title => "Enemy: {name}", fields => [{"Faction", faction}, {"Health", health},
                                            {"Shield", shield}, {"Armor", armor},
                                            {"Resistances", resistances}, {"Drops", dropCount},
                                            {"Description", description}, {"Unique", uniqueName}]};
block_spec(drop) ->
    #{title => "Drop: {item}", fields => [{"Enemy", enemy}, {"Chance", chance},
                                           {"Rarity", rarity}, {"Table", table},
                                           {"Version", sourceVersion}]}.
