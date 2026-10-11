%%%-------------------------------------------------------------------
%% Daemon-owned metadata for game identities absent from public catalogs.
%%%-------------------------------------------------------------------
-module(wfcli_builtin_metadata).

-export([equipment/1, component_name/2, reward/1]).

-doc "Return attributed equipment metadata for one internal game identity.".
-spec equipment(binary()) -> map() | undefined.
equipment(Identity) when is_binary(Identity) ->
    maps:get(Identity, equipment_entries(), undefined);
equipment(_Identity) -> undefined.

-doc "Apply a daemon-owned component display-name override when one exists.".
-spec component_name(binary(), binary()) -> {binary(), binary()}.
component_name(<<"/Lotus/Types/Items/MiscItems/Forma">>, _Default) ->
    {<<"Forma Blueprint">>, <<"builtin">>};
component_name(_Identity, Default) ->
    {Default, <<"catalog">>}.

-doc "Return built-in reward metadata for rewards not fully represented upstream.".
-spec reward(binary()) -> map() | undefined.
reward(Name) when is_binary(Name) ->
    case binary:match(string:lowercase(Name), <<"forma blueprint">>) of
        nomatch -> undefined;
        _ -> #{name => <<"Forma Blueprint">>, platinum => 2,
               asset => #{<<"id">> => <<"builtin:forma">>,
                          <<"source">> => <<"builtin">>,
                          <<"image_name">> => <<"forma.png">>}}
    end;
reward(_Name) -> undefined.

equipment_entries() ->
    #{
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/AdarzaPetWeapon">> =>
          entry(<<"Adarza Kavat Claws">>,
                <<"/Lotus/Types/Game/CatbrowPet/MirrorCatbrowPetPowerSuit">>),
      <<"/Lotus/Powersuits/Operator/AdultOperatorSuitRemaster">> =>
          entry(<<"Drifter">>),
      <<"/Lotus/Powersuits/Yareli/BoardPrimeSuit">> =>
          entry(<<"Merulina Prime">>,
                <<"/Lotus/Upgrades/Skins/Yareli/PrimeMerulinaBoard">>),
      <<"/Lotus/Powersuits/Operator/ChildOperatorSuitRemaster">> =>
          entry(<<"Operator">>),
      <<"/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness">> =>
          entry(<<"Plexus">>),
      <<"/Lotus/Types/Friendly/PlayerControllable/Weapons/DuviriDualSwords">> =>
          entry(<<"Sun & Moon">>,
                <<"/Lotus/Types/Friendly/PlayerControllable/Weapons/DuviriDualSwordsWeapon">>),
      <<"/Lotus/Types/NeutralCreatures/ErsatzHorse/ErsatzHorsePowerSuit">> =>
          entry(<<"Kaithe">>, <<"/Lotus/Types/Restoratives/ErsatzSummon">>),
      <<"/Lotus/Powersuits/Temple/ExaltedGuitar">> =>
          entry(<<"Lizzie">>,
                <<"/Lotus/Types/Items/ShipDecos/LisetPropShawzinTempleGuitar">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/HelminthPetWeapon">> =>
          entry(<<"Helminth Charger Claws">>,
                <<"/Lotus/Types/Game/KubrowPet/ChargerKubrowPetPowerSuit">>),
      <<"/Lotus/Types/Vehicles/Motorcycle/MotorcyclePowerSuit">> =>
          entry(<<"Atomicycle">>, <<"/Lotus/Types/Restoratives/MotorcycleSummon">>),
      <<"/Lotus/Weapons/Sentients/OperatorAmplifiers/OperatorAmpWeapon">> =>
          entry(<<"Operator Amp">>),
      <<"/Lotus/Weapons/Sentients/OperatorAmplifiers/SentTrainingAmplifier/OperatorTrainingAmpWeapon">> =>
          entry(<<"Mote Amp">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/PanzerVulpaphylaPetWeapon">> =>
          entry(<<"Panzer Vulpaphyla Claws">>,
                <<"/Lotus/Types/Friendly/Pets/CreaturePets/ArmoredInfestedCatbrowPetPowerSuit">>),
      <<"/Lotus/Types/Game/CrewShip/CrewMember/RedVeilCrewMemberGeneratorMediumVersionTwo">> =>
          entry(<<"Red Veil Crew Member">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/SahasaPetWeapon">> =>
          entry(<<"Sahasa Kubrow Claws">>,
                <<"/Lotus/Types/Game/KubrowPet/AdventurerKubrowPetPowerSuit">>),
      <<"/Lotus/Powersuits/Wraith/SevagothShadowPrime">> =>
          entry(<<"Sevagoth Prime's Shadow">>,
                <<"/Lotus/Upgrades/Skins/Wraith/SevagothPrimeShadowSkin">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/SmeetaPetWeapon">> =>
          entry(<<"Smeeta Kavat Claws">>,
                <<"/Lotus/Types/Game/CatbrowPet/CheshireCatbrowPetPowerSuit">>),
      <<"/Lotus/Types/Game/CrewShip/CrewMember/SteelMeridianCrewMemberGeneratorStrong">> =>
          entry(<<"Steel Meridian Crew Member">>),
      <<"/Lotus/Weapons/Tenno/HackingDevices/TnHackingDevice/TnHackingDeviceWeapon">> =>
          entry(<<"Parazon">>, <<"/Lotus/Types/Items/Emotes/ParazonEmote">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/VenariPetWeapon">> =>
          entry(<<"Venari Claws">>, <<"/Lotus/Powersuits/Khora/Kavat/KhoraKavatPowerSuit">>),
      <<"/Lotus/Types/Friendly/Pets/BeastWeapons/VenariPrimePetWeapon">> =>
          entry(<<"Venari Prime Claws">>,
                <<"/Lotus/Powersuits/Khora/Kavat/KhoraPrimeKavatPowerSuit">>)
     }.

entry(Name) -> #{name => Name, source => <<"builtin">>}.
entry(Name, CatalogAlias) ->
    (entry(Name))#{catalog_alias => CatalogAlias}.
