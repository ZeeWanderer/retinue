#include "player_identity_widget.h"

#include <QFontMetrics>
#include <QJsonObject>
#include <QLabel>
#include <QPixmap>
#include <QResizeEvent>
#include <QVBoxLayout>

#include "app_controller.h"
#include "thumbnail_widget.h"

PlayerIdentityWidget::PlayerIdentityWidget(AppController *controller,
                                           QWidget *parent)
    : QWidget(parent), controller_(controller), icon_(new ThumbnailWidget),
      rank_(new QLabel), name_(new QLabel(this)), playerName_("Player") {
  setObjectName("playerIdentity");

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(5, 5, 5, 6);
  layout->setSpacing(1);

  auto *emblem = new QWidget;
  emblem->setObjectName("playerEmblem");
  emblem->setFixedSize(50, 50);
  icon_->setParent(emblem);
  icon_->setObjectName("playerEmblemIcon");
  icon_->setGeometry(0, 0, 50, 50);
  rank_->setParent(emblem);
  rank_->setObjectName("playerRankBadge");
  rank_->setAlignment(Qt::AlignCenter);
  rank_->raise();
  layout->addWidget(emblem, 0, Qt::AlignHCenter);

  name_->setObjectName("playerName");
  name_->setAlignment(Qt::AlignCenter);
  name_->setToolTip(playerName_);
  layout->addWidget(name_);

  connect(controller_, &AppController::playerProfileChanged, this,
          [this] { updateProfile(); });
  connect(controller_, &AppController::assetsChanged, this,
          [this](const QStringList &ids) {
            const QString rankAssetId = controller_->playerProfile()
                                            .value("rank_asset")
                                            .toObject()
                                            .value("id")
                                            .toString();
            if (ids.contains(rankAssetId)) {
              updateProfile();
            }
          });
  updateProfile();
}

void PlayerIdentityWidget::resizeEvent(QResizeEvent *event) {
  QWidget::resizeEvent(event);
  updateName();
}

void PlayerIdentityWidget::updateProfile() {
  const QJsonObject profile = controller_->playerProfile();
  playerName_ = profile.value("player_name").toString().trimmed();
  if (playerName_.isEmpty()) {
    playerName_ = "Player";
  }
  const QJsonValue level = profile.value("player_level");
  rank_->setText(level.isDouble() ? QString::number(level.toInt()) : "--");
  rank_->adjustSize();
  rank_->move(47 - rank_->width(), 47 - rank_->height());
  const QString rankAssetId =
      profile.value("rank_asset").toObject().value("id").toString();
  const auto asset = controller_->assetRef(rankAssetId);
  icon_->setAsset(asset.isValid()
                      ? asset
                      : wfgui::AssetRef::embedded(
                            "mastery_rank", ":/resources/ui/mastery_rank.png"));
  name_->setToolTip(playerName_);
  updateName();
}

void PlayerIdentityWidget::updateName() {
  const int width = qMax(1, name_->width() - 2);
  name_->setText(QFontMetrics(name_->font())
                     .elidedText(playerName_, Qt::ElideRight, width));
}
