module;

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <utility>
#include <vector>

export module wfgui.relic_data;

export namespace wfgui {

struct Relic {
  struct Refinement {
    QString name;
    int amountOwned = 0;
    int expectedPlatinum = 0;
    int expectedDucats = 0;
    bool hasPrice = false;
    bool priceComplete = false;
  };

  struct Reward {
    QString name;
    QString rarity;
    QString assetId;
    QString assetPath;
    int platinum = 0;
    int ducats = 0;
    int owned = 0;
    double chance = 0;
    bool hasPrice = false;
  };

  QString id;
  QString name;
  QString era;
  QString refinement;
  QString assetId;
  QString assetPath;
  int amountOwned = 0;
  bool vaulted = false;
  bool favorite = false;
  bool hasPrice = false;
  int expectedPlatinum = 0;
  int expectedDucats = 0;
  bool priceComplete = false;
  std::vector<Refinement> refinements;
  std::vector<Reward> rewards;
};

struct RelicData {
  std::vector<Relic> relics;
  int traceCount = 0;
};

bool parseRelicData(const QJsonObject &data, RelicData &result,
                    QString *error = nullptr) {
  const QJsonValue itemsValue = data.value("items");
  if (!itemsValue.isArray()) {
    if (error != nullptr) {
      *error = "daemon returned malformed relic planner data";
    }
    return false;
  }

  RelicData next;
  const QJsonArray items = itemsValue.toArray();
  next.relics.reserve(static_cast<std::size_t>(items.size()));
  for (const QJsonValue &value : items) {
    if (!value.isObject()) {
      continue;
    }
    const QJsonObject item = value.toObject();
    const QString name = item.value("name").toString();
    if (name.isEmpty()) {
      continue;
    }
    const QJsonValue platinum = item.value("expected_platinum");
    Relic relic{
        .id = item.value("id").toString(),
        .name = name,
        .era = item.value("era").toString(),
        .refinement = item.value("refinement").toString(),
        .assetId = item.value("asset").toObject().value("id").toString(),
        .assetPath = {},
        .amountOwned = item.value("amount_owned").toInt(),
        .vaulted = item.value("vaulted").toBool(),
        .favorite = item.value("favorite").toBool(),
        .hasPrice = platinum.isDouble(),
        .expectedPlatinum = platinum.toInt(),
        .expectedDucats = item.value("expected_ducats").toInt(),
        .priceComplete = item.value("price_complete").toBool(),
        .refinements = {},
        .rewards = {},
    };
    const QJsonArray refinements = item.value("refinements").toArray();
    relic.refinements.reserve(static_cast<std::size_t>(refinements.size()));
    for (const QJsonValue &refinementValue : refinements) {
      const QJsonObject refinement = refinementValue.toObject();
      const QJsonValue refinementPrice =
          refinement.value("expected_platinum");
      relic.refinements.push_back({
          .name = refinement.value("refinement").toString(),
          .amountOwned = refinement.value("amount_owned").toInt(),
          .expectedPlatinum = refinementPrice.toInt(),
          .expectedDucats = refinement.value("expected_ducats").toInt(),
          .hasPrice = refinementPrice.isDouble(),
          .priceComplete = refinement.value("price_complete").toBool(),
      });
    }
    const QJsonArray rewards = item.value("rewards").toArray();
    relic.rewards.reserve(static_cast<std::size_t>(rewards.size()));
    for (const QJsonValue &rewardValue : rewards) {
      const QJsonObject reward = rewardValue.toObject();
      const QJsonValue rewardPrice = reward.value("platinum");
      relic.rewards.push_back({
          .name = reward.value("name").toString(),
          .rarity = reward.value("rarity").toString(),
          .assetId = reward.value("asset").toObject().value("id").toString(),
          .assetPath = {},
          .platinum = rewardPrice.toInt(),
          .ducats = reward.value("ducats").toInt(),
          .owned = reward.value("owned").toInt(),
          .chance = reward.value("chance").toDouble(),
          .hasPrice = rewardPrice.isDouble(),
      });
    }
    next.relics.push_back(std::move(relic));
  }
  next.traceCount = data.value("trace_count").toInt();
  result = std::move(next);
  return true;
}

} // namespace wfgui
