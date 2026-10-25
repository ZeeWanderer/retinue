#pragma once

#include <QColor>
#include <QWidget>

#include "asset_ref.h"

class ThumbnailWidget final : public QWidget {
public:
  explicit ThumbnailWidget(QWidget *parent = nullptr);
  void setAsset(const wfgui::AssetRef &asset);
  void setImageBounds(QSize bounds);
  void setTint(QColor color);

protected:
  void paintEvent(QPaintEvent *event) override;

private:
  wfgui::AssetRef asset_;
  QSize bounds_;
  QColor tint_;
};
