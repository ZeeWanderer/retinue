#include "asset_ref.h"

#include <QJsonObject>

#include <utility>

namespace wfgui {

bool AssetRef::isValid() const { return !id.isEmpty() && !path.isEmpty(); }

bool AssetRef::isPersistent() const {
  return isValid() && !source.isEmpty() && !imageName.isEmpty() &&
         !digest.isEmpty() && !path.startsWith(":/");
}

AssetRef AssetRef::fromJson(const QJsonObject &value) {
  if (!value.value("ok").toBool()) {
    return {};
  }
  return {
      .id = value.value("id").toString(),
      .source = value.value("source").toString(),
      .imageName = value.value("image_name").toString(),
      .path = value.value("path").toString(),
      .digest = value.value("digest").toString(),
      .mediaType = value.value("media_type").toString(),
      .size = value.value("size").toInteger(),
      .stale = value.value("stale").toBool(),
  };
}

AssetRef AssetRef::embedded(QString id, QString path) {
  return {
      .id = std::move(id),
      .source = "builtin",
      .imageName = path,
      .path = std::move(path),
      .digest = {},
      .mediaType = {},
      .size = 0,
      .stale = false,
  };
}

} // namespace wfgui
