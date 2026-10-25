#pragma once

#include <QAbstractListModel>
#include <QJsonArray>
#include <QJsonObject>

class BuildGroupModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    IdRole = Qt::UserRole + 1,
    NameRole,
    DefinitionIdRole,
    InstanceIdRole,
    RevisionRole,
    MemberCountRole,
    SourceCountRole,
    ConfigCountRole,
    UpdatedAtRole,
    MembersRole,
    BaselineRole,
    RawRole,
  };

  explicit BuildGroupModel(QObject *parent = nullptr);

  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  QHash<int, QByteArray> roleNames() const override;

  bool replace(const QJsonObject &data, QString *error = nullptr);
  bool upsert(const QJsonObject &group, QString *error = nullptr);
  void remove(const QString &id);
  void clear();
  QJsonObject group(const QString &id) const;

private:
  static bool valid(const QJsonObject &group);
  void sortGroups();

  QJsonArray groups_;
};
