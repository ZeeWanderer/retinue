#include "build_topology_widget.h"

#include <QFrame>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QHash>
#include <QJsonArray>
#include <QLabel>
#include <QSet>
#include <QVBoxLayout>

#include "app_controller.h"
#include "arcane_card_widget.h"
#include "mod_card_widget.h"
#include "widget_capture.h"

namespace {
QString pathName(const QString &path) {
  const qsizetype slash = path.lastIndexOf('/');
  return slash >= 0 ? path.mid(slash + 1) : path;
}

QJsonObject configAt(const QJsonArray &configs, int configIndex) {
  for (const QJsonValue &value : configs) {
    const QJsonObject config = value.toObject();
    if (config.value("config_index").toInt(-1) == configIndex) {
      return config;
    }
  }
  return {};
}

QHash<QString, QJsonObject> indexUpgrades(const QJsonArray &upgrades) {
  QHash<QString, QJsonObject> result;
  for (const QJsonValue &value : upgrades) {
    const QJsonObject upgrade = value.toObject();
    const QString id = upgrade.value("topology_slot").toString();
    if (!id.isEmpty()) {
      result.insert(id, upgrade);
    }
  }
  return result;
}

QHash<QString, QString> polaritiesBySlot(const QJsonArray &polarities) {
  QHash<QString, QString> result;
  for (const QJsonValue &value : polarities) {
    const QJsonObject polarity = value.toObject();
    const QString slot = polarity.value("slot_id").toString();
    if (!slot.isEmpty()) {
      result.insert(slot, polarity.value("polarity").toString("none"));
    }
  }
  return result;
}

QHash<QString, QJsonObject> shardsBySlot(const QJsonArray &shards) {
  QHash<QString, QJsonObject> result;
  for (const QJsonValue &value : shards) {
    const QJsonObject shard = value.toObject();
    const QString id = shard.value("slot_id").toString();
    if (!id.isEmpty()) {
      result.insert(id, shard.value("upgrade").toObject());
    }
  }
  return result;
}

QString upgradeName(const QJsonObject &upgrade) {
  const QString name = upgrade.value("name").toString();
  return name.isEmpty() ? pathName(upgrade.value("item_type").toString())
                        : name;
}

QString upgradeMeta(const QJsonObject &upgrade) {
  QStringList parts;
  if (upgrade.value("rank").isDouble()) {
    const int rank = upgrade.value("rank").toInt();
    const int maximum = upgrade.value("max_rank").toInt(-1);
    parts.append(maximum >= rank ? QString("Rank %1/%2").arg(rank).arg(maximum)
                                 : QString("Rank %1").arg(rank));
  }
  if (upgrade.value("drain").isDouble()) {
    const int drain = upgrade.value("drain").toInt();
    const int effective = upgrade.value("effective_drain").toInt(drain);
    parts.append(effective == drain
                     ? QString("Drain %1").arg(drain)
                     : QString("Drain %1 (%2 base)").arg(effective).arg(drain));
  }
  return parts.join("  ·  ");
}

void requestUpgradeAssets(AppController *controller,
                          const QJsonArray &upgrades) {
  if (!controller) {
    return;
  }
  QJsonArray assets;
  QSet<QString> ids;
  for (const QJsonValue &value : upgrades) {
    const QJsonObject asset = value.toObject().value("asset").toObject();
    const QString id = asset.value("id").toString();
    if (!id.isEmpty() && !ids.contains(id)) {
      ids.insert(id);
      assets.append(asset);
    }
  }
  controller->resolveAssets(assets);
}

void addSlotHeader(QVBoxLayout *layout, const QJsonObject &slot,
                   const QString &polarity) {
  auto *header = new QHBoxLayout;
  header->setSpacing(5);
  auto *label = new QLabel(slot.value("label").toString());
  label->setObjectName("buildSlotLabel");
  header->addWidget(label);
  header->addStretch();
  if (!polarity.isEmpty() && polarity != "none") {
    auto *polarityLabel = new QLabel;
    polarityLabel->setObjectName("buildSlotPolarity");
    polarityLabel->setProperty("polarity", polarity);
    polarityLabel->setToolTip("Slot polarity: " + polarity);
    polarityLabel->setPixmap(
        QIcon(wfgui::modPolarityPixmap(polarity, QColor("#c4cada")))
            .pixmap(QSize(16, 16), polarityLabel->devicePixelRatioF()));
    header->addWidget(polarityLabel);
  }
  layout->addLayout(header);
}

QWidget *modCard(AppController *controller, const QJsonObject &slot,
                 const QJsonObject &upgrade, const QString &polarity) {
  return new wfgui::ModCardWidget(controller, slot, upgrade, polarity);
}

QWidget *arcaneCard(AppController *controller, const QJsonObject &slot,
                    const QJsonObject &upgrade) {
  return new wfgui::ArcaneCardWidget(controller, slot, upgrade);
}

QFrame *schematicCard(const QJsonObject &slot, const QJsonObject &upgrade,
                      const QString &polarity, const QJsonObject &shard) {
  auto *card = new QFrame;
  card->setObjectName("buildTopologySlot");
  card->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
  card->setFixedWidth(184);
  card->setFixedHeight(slot.value("role").toString() == "shard" ? 74 : 96);
  auto *layout = new QVBoxLayout(card);
  layout->setContentsMargins(8, 6, 8, 7);
  layout->setSpacing(2);
  addSlotHeader(layout, slot, polarity);

  QString name;
  QString meta;
  if (!shard.isEmpty()) {
    name = pathName(shard.value("upgrade_type")
                        .toString(shard.value("UpgradeType").toString()));
    meta = "Installed";
  } else if (!upgrade.isEmpty()) {
    name = upgradeName(upgrade);
    meta = upgradeMeta(upgrade);
  } else if (slot.contains("unlocked") && !slot.value("unlocked").toBool()) {
    name = "Locked";
  } else {
    name = "Empty";
  }
  auto *nameLabel = new QLabel(name);
  nameLabel->setObjectName("buildSlotName");
  nameLabel->setWordWrap(true);
  layout->addWidget(nameLabel, 1);
  if (!meta.isEmpty()) {
    auto *metaLabel = new QLabel(meta);
    metaLabel->setObjectName("buildSlotMeta");
    layout->addWidget(metaLabel);
  }
  return card;
}

QWidget *slotCard(AppController *controller, const QJsonObject &slot,
                  const QJsonObject &upgrade, const QString &polarity,
                  const QJsonObject &shard) {
  QWidget *card;
  const QString role = slot.value("role").toString("mod");
  if (role == "arcane") {
    card = arcaneCard(controller, slot, upgrade);
  } else if (shard.isEmpty() && (role == "mod" || role == "aura" ||
                                 role == "exilus" || role == "stance")) {
    card = modCard(controller, slot, upgrade, polarity);
  } else {
    card = schematicCard(slot, upgrade, polarity, shard);
  }
  wfgui::setCaptureItem(card);
  return card;
}

QJsonArray sourceUpgrades(const QJsonObject &revision) {
  return revision.value("presentation").toObject().value("upgrades").toArray();
}
} // namespace

BuildTopologyWidget::BuildTopologyWidget(AppController *controller,
                                         QWidget *parent)
    : QWidget(parent), layout_(new QVBoxLayout(this)), controller_(controller) {
  setObjectName("buildTopology");
  wfgui::setCaptureTarget(this, "build-planner.topology", true);
  layout_->setContentsMargins(0, 0, 0, 0);
  layout_->setSpacing(8);
  clear();
}

void BuildTopologyWidget::setPlayerInstance(const QJsonObject &instance,
                                            int configIndex) {
  const QJsonObject config =
      configAt(instance.value("configs").toArray(), configIndex);
  render(instance.value("topology").toObject(),
         config.value("upgrade_slots").toArray(),
         instance.value("effective_polarities").toArray(),
         instance.value("shard_slots").toArray(),
         config.value("ability_override").toArray());
}

void BuildTopologyWidget::setPlayerSnapshot(const QJsonObject &snapshot) {
  const QJsonObject config = snapshot.value("config").toObject();
  render(snapshot.value("topology").toObject(),
         config.value("upgrade_slots").toArray(),
         snapshot.value("effective_polarities").toArray(),
         snapshot.value("shard_slots").toArray(),
         config.value("ability_override").toArray());
}

void BuildTopologyWidget::setSourceRevision(const QJsonObject &revision,
                                            const QJsonObject &baseline) {
  QJsonObject topology = baseline.value("topology").toObject();
  if (topology.isEmpty()) {
    topology =
        revision.value("presentation").toObject().value("topology").toObject();
  }
  render(
      topology, sourceUpgrades(revision),
      baseline.value("effective_polarities").toArray(),
      baseline.value("shard_slots").toArray(),
      revision.value("content").toObject().value("ability_override").toArray());
}

void BuildTopologyWidget::setPlanResult(const QJsonObject &result,
                                        const QJsonObject &baseline) {
  render(baseline.value("topology").toObject(), {},
         result.value("final_polarities").toArray(),
         baseline.value("shard_slots").toArray(), {});
}

void BuildTopologyWidget::clear() {
  resetBody();
  auto *empty = new QLabel("Select a configuration or build.", body_);
  empty->setObjectName("emptyState");
  empty->setAlignment(Qt::AlignCenter);
  qobject_cast<QVBoxLayout *>(body_->layout())->addWidget(empty, 1);
}

void BuildTopologyWidget::render(const QJsonObject &topology,
                                 const QJsonArray &upgrades,
                                 const QJsonArray &polarities,
                                 const QJsonArray &shards,
                                 const QJsonArray &abilityOverrides) {
  resetBody();
  requestUpgradeAssets(controller_, upgrades);
  auto *bodyLayout = qobject_cast<QVBoxLayout *>(body_->layout());
  const auto upgradeIndex = indexUpgrades(upgrades);
  const auto polarityBySlot = polaritiesBySlot(polarities);
  const auto shardBySlot = shardsBySlot(shards);

  for (const QJsonValue &regionValue : topology.value("regions").toArray()) {
    const QJsonObject region = regionValue.toObject();
    auto *regionWidget = new QWidget(body_);
    regionWidget->setObjectName("buildTopologyRegion");
    regionWidget->setProperty("regionId", region.value("id").toString());
    auto *regionLayout = new QVBoxLayout(regionWidget);
    regionLayout->setContentsMargins(0, 0, 0, 0);
    regionLayout->setSpacing(4);
    auto *label = new QLabel(region.value("label").toString());
    label->setObjectName("buildRegionTitle");
    regionLayout->addWidget(label);

    auto *gridWidget = new QWidget(regionWidget);
    gridWidget->setObjectName("buildRegionGrid");
    gridWidget->setProperty("regionId", region.value("id").toString());
    gridWidget->setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Preferred);
    auto *grid = new QGridLayout(gridWidget);
    grid->setContentsMargins(14, 0, 14, 0);
    grid->setHorizontalSpacing(region.value("id").toString() == "arcanes" ? 0
                                                                          : 4);
    grid->setVerticalSpacing(0);
    const int columns = qMax(1, region.value("columns").toInt(1));
    int index = 0;
    for (const QJsonValue &slotValue : region.value("slots").toArray()) {
      const QJsonObject slot = slotValue.toObject();
      const QString slotId = slot.value("id").toString();
      grid->addWidget(slotCard(controller_, slot, upgradeIndex.value(slotId),
                               polarityBySlot.value(slotId),
                               shardBySlot.value(slotId)),
                      index / columns, index % columns);
      ++index;
    }
    regionLayout->addWidget(gridWidget, 0, Qt::AlignHCenter);
    bodyLayout->addWidget(regionWidget);
  }

  if (!abilityOverrides.isEmpty()) {
    QStringList names;
    for (const QJsonValue &value : abilityOverrides) {
      const QString name = pathName(value.toString());
      if (!name.isEmpty()) {
        names.append(name);
      }
    }
    if (!names.isEmpty()) {
      auto *override = new QLabel("Ability override: " + names.join(", "));
      override->setObjectName("buildAbilityOverride");
      override->setWordWrap(true);
      bodyLayout->addWidget(override);
    }
  }
  bodyLayout->addStretch();
}

void BuildTopologyWidget::resetBody() {
  if (body_) {
    layout_->removeWidget(body_);
    body_->deleteLater();
  }
  body_ = new QWidget(this);
  body_->setObjectName("buildTopologyBody");
  auto *bodyLayout = new QVBoxLayout(body_);
  bodyLayout->setContentsMargins(0, 0, 0, 0);
  bodyLayout->setSpacing(6);
  layout_->addWidget(body_);
}
