#pragma once

#include <QJsonObject>
#include <QWidget>

#include <functional>

#include "asset_ref.h"

class QLabel;
class QToolButton;
class QGraphicsOpacityEffect;
class ThumbnailWidget;

struct MarketOrderCardActions {
  std::function<void()> visibility;
  std::function<void()> edit;
  std::function<void()> add;
  std::function<void()> close;
  std::function<void()> remove;
  std::function<void()> listings;
};

class MarketOrderCard final : public QWidget {
public:
  MarketOrderCard(const QJsonObject &order, const QJsonObject &item,
                  const QJsonObject &quote, int owned,
                  MarketOrderCardActions actions, QWidget *parent = nullptr);
  void updateOrder(const QJsonObject &order, const QJsonObject &item,
                   const QJsonObject &quote, int owned,
                   MarketOrderCardActions actions);
  void setAsset(const wfgui::AssetRef &asset);

private:
  QToolButton *visibility_;
  QWidget *top_;
  QWidget *body_;
  QGraphicsOpacityEffect *opacity_;
  ThumbnailWidget *image_;
  QLabel *name_;
  QLabel *owned_;
  QLabel *warning_;
  QLabel *side_;
  QLabel *quantity_;
  QLabel *price_;
  QLabel *extra_;
  QLabel *comparisonTitle_;
  QLabel *comparison_;
  MarketOrderCardActions actions_;
  QJsonObject order_;
  QJsonObject item_;
  QJsonObject quote_;
  int ownedQuantity_ = -2;
};
