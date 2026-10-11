%%%-------------------------------------------------------------------
%% Shared Codex and WFCD result-column contract.
%%%-------------------------------------------------------------------
-module(wf_knowledge_schema).

-export([table_spec/1, columns_spec/0, column_spec/1, columns_for_kind/1]).

-type kind() :: codex | enemy | drop.
-type column_spec() :: map().

-spec columns_for_kind(kind()) -> [atom()].
columns_for_kind(codex) -> [name, category, description, secret, file];
columns_for_kind(enemy) -> [name, faction, health, shield, armor, resistances, dropCount];
columns_for_kind(drop) -> [item, enemy, chance, rarity, table].

-spec table_spec(kind()) -> #{columns := [atom()], specs := [column_spec()]}.
table_spec(Kind) ->
    Cols = columns_for_kind(Kind),
    #{columns => Cols, specs => [column_spec(C) || C <- Cols]}.

-spec columns_spec() -> [column_spec()].
columns_spec() ->
    [#{key => name, label => "Name", role => name},
     #{key => category, label => "Category", role => type},
     #{key => description, label => "Description", role => details, optional => true},
     #{key => secret, label => "Secret", role => flags, optional => true},
     #{key => excluded, label => "Excluded", role => flags, optional => true},
     #{key => file, label => "File", role => details, optional => true},
     #{key => faction, label => "Faction", role => type},
     #{key => health, label => "Health", role => stat, kind => numeric},
     #{key => shield, label => "Shield", role => stat, kind => numeric},
     #{key => armor, label => "Armor", role => stat, kind => numeric},
     #{key => resistances, label => "Resistances", role => details, optional => true},
     #{key => dropCount, label => "Drops", role => stat, kind => numeric},
     #{key => item, label => "Item", role => name},
     #{key => enemy, label => "Enemy", role => location},
     #{key => chance, label => "Chance", role => stat, kind => numeric},
     #{key => rarity, label => "Rarity", role => stat},
     #{key => table, label => "Table", role => details, optional => true},
     #{key => sourceVersion, label => "Version", role => id, optional => true},
     #{key => uniqueName, label => "Unique", role => id, optional => true}].

-spec column_spec(atom()) -> column_spec().
column_spec(Key) ->
    case lists:dropwhile(fun(S) -> maps:get(key, S) =/= Key end, columns_spec()) of
        [Spec | _] -> Spec;
        [] -> #{key => Key, label => atom_to_list(Key)}
    end.
