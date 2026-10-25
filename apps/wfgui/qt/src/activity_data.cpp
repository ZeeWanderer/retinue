#include "activity_data.h"

#include <QDateTime>
#include <QJsonObject>

namespace wfgui {

QJsonArray activeFissures(const QJsonArray &fissures, qint64 now) {
  QJsonArray active;
  for (const QJsonValue &value : fissures) {
    const QDateTime expiry = QDateTime::fromString(
        value.toObject().value("expiry").toString(), Qt::ISODate);
    if (!expiry.isValid() || expiry.toMSecsSinceEpoch() > now) {
      active.append(value);
    }
  }
  return active;
}

QString relicEraForFissureTier(const QString &tier) {
  if (tier == "Lith" || tier == "Meso" || tier == "Neo" || tier == "Axi" ||
      tier == "Requiem") {
    return tier.toLower();
  }
  return "all";
}

} // namespace wfgui
