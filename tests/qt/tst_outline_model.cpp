#include "models/outline_model.h"

#include <QMimeData>
#include <QTest>

class FakeRustOutline final : public QObject
{
  Q_OBJECT
  Q_PROPERTY(int node_count READ nodeCount CONSTANT)

public:
  int nodeCount() const { return 10'000; }
  Q_INVOKABLE QString nodeTitle(qint32 row) const { return QStringLiteral("Node %1").arg(row); }
  Q_INVOKABLE QString nodeId(qint32 row) const { return QStringLiteral("node-%1").arg(row); }
  Q_INVOKABLE qint32 nodeDepth(qint32 row) const { return row == 0 ? 0 : 2; }
  Q_INVOKABLE qint32 nodeParent(qint32 row) const { return row == 0 ? -1 : 0; }
  Q_INVOKABLE bool nodeIsRoot(qint32 row) const { return row == 0; }
  Q_INVOKABLE bool nodeIsGroup(qint32) const { return false; }
  Q_INVOKABLE qint32 nodeWordCount(qint32 row) const { return row + 1; }
  Q_INVOKABLE bool nodeIncludeInCompile(qint32) const { return true; }
  Q_INVOKABLE bool moveNode(const QString& source, const QString& target, const QString& placement)
  {
    movedSource = source;
    movedTarget = target;
    movedPlacement = placement;
    return true;
  }

  QString movedSource;
  QString movedTarget;
  QString movedPlacement;

signals:
  void revisionChanged();
};

class OutlineModelTest final : public QObject
{
  Q_OBJECT

private slots:
  void lazilyQueriesRustShapedSource()
  {
    FakeRustOutline source;
    OutlineModel model;
    model.setSource(&source);
    QCOMPARE(model.rowCount(), 10'000);
    QCOMPARE(model.data(model.index(9'999), OutlineModel::TitleRole).toString(),
             QStringLiteral("Node 9999"));
    QCOMPARE(model.data(model.index(9'999), OutlineModel::DepthRole).toInt(), 2);
    QCOMPARE(model.data(model.index(9'999), OutlineModel::ParentIdRole).toInt(), 0);
    QCOMPARE(model.data(model.index(4), OutlineModel::WordCountRole).toInt(), 5);
  }

  void typedDragPayloadUsesTheSameMoveContractAsTheBinder()
  {
    FakeRustOutline source;
    OutlineModel model;
    model.setSource(&source);
    const auto root = model.index(0);
    QVERIFY(!(model.flags(root) & Qt::ItemIsDragEnabled));

    auto* mime = model.mimeData({ model.index(3) });
    QVERIFY(mime);
    QCOMPARE(mime->data("application/x-parchmint-node-id"), QByteArrayLiteral("node-3"));
    QVERIFY(model.dropMimeData(mime, Qt::MoveAction, 2, 0, QModelIndex()));
    QCOMPARE(source.movedSource, QStringLiteral("node-3"));
    QCOMPARE(source.movedTarget, QStringLiteral("node-2"));
    QCOMPARE(source.movedPlacement, QStringLiteral("before"));
    delete mime;
  }
};

QTEST_MAIN(OutlineModelTest)
#include "tst_outline_model.moc"
