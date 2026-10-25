#pragma once

#include <QImage>
#include <QMutex>
#include <QSize>
#include <QString>

#include "asset_ref.h"

namespace wfgui {

struct DerivativeCacheStats {
  QString path;
  qint64 files = 0;
  qint64 bytes = 0;
};

class DerivativeCache final {
public:
  explicit DerivativeCache(QString root = {});

  [[nodiscard]] QString root() const;
  void registerAsset(const AssetRef &asset);
  [[nodiscard]] QImage load(const AssetRef &asset,
                            const QSize &pixelBounds) const;
  [[nodiscard]] bool store(const AssetRef &asset, const QSize &pixelBounds,
                           const QImage &image);
  [[nodiscard]] DerivativeCacheStats stats() const;
  [[nodiscard]] bool clear();

private:
  [[nodiscard]] QString originDirectory(const AssetRef &asset) const;
  [[nodiscard]] QString digestDirectory(const AssetRef &asset) const;
  [[nodiscard]] QString derivativePath(const AssetRef &asset,
                                       const QSize &pixelBounds) const;
  [[nodiscard]] QString digestKey(const AssetRef &asset) const;
  [[nodiscard]] bool isCurrent(const AssetRef &asset) const;

  QString root_;
  mutable QMutex mutex_;
};

} // namespace wfgui
