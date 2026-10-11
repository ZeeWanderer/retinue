%%%-------------------------------------------------------------------
%% Mod lookup from ExportUpgrades metadata for missing polarity/cost.
%%%-------------------------------------------------------------------
-module(wf_forma_mod_db).

-export([lookup/1]).

lookup(Name) ->
    wf_mod_catalog:lookup_by_name(Name).
