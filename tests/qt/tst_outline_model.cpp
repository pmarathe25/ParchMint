#include "models/cards_model.h"
#include "models/outline_model.h"

#include <QMimeData>
#include <QTest>

class FakeRustOutline final : public QObject
{
  Q_OBJECT
  Q_PROPERTY(int node_count READ nodeCount CONSTANT)
  Q_PROPERTY(qulonglong structure_revision READ structureRevision NOTIFY structureRevisionChanged)
  Q_PROPERTY(qulonglong content_revision READ contentRevision NOTIFY contentRevisionChanged)
  Q_PROPERTY(qulonglong presentation_revision READ presentationRevision NOTIFY presentationRevisionChanged)

public:
  int nodeCount() const { return 10'000; }
  qulonglong structureRevision() const { return 1; }
  qulonglong contentRevision() const { return 1; }
  qulonglong presentationRevision() const { return 1; }
  Q_INVOKABLE QString nodeTitle(qint32 row) const { return QStringLiteral("Node %1").arg(row); }
  Q_INVOKABLE QString nodeId(qint32 row) const { return QStringLiteral("node-%1").arg(row); }
  Q_INVOKABLE qint32 nodeDepth(qint32 row) const { return row == 0 ? 0 : 2; }
  Q_INVOKABLE qint32 nodeParent(qint32 row) const { return row == 0 ? -1 : 0; }
  Q_INVOKABLE bool nodeIsRoot(qint32 row) const { return row == 0; }
  Q_INVOKABLE bool nodeIsGroup(qint32) const { return false; }
  Q_INVOKABLE qint32 nodeWordCount(qint32 row) const { return row + 1; }
  Q_INVOKABLE bool nodeIncludeInCompile(qint32) const { return true; }
  Q_INVOKABLE QString nodeRowJson(qint32 row) const
  {
    ++rowRequests;
    const bool researchRoot = row == 2;
    const bool research = researchRoot || row == 3;
    const bool root = row == 0 || researchRoot;
    const int parent = root ? -1 : (research ? 2 : 0);
    const auto title = row == 0 ? QStringLiteral("Draft")
      : researchRoot ? QStringLiteral("References") : QStringLiteral("Node %1").arg(row);
    return QStringLiteral(R"({"title":"%1","nodeId":"node-%2","depth":%3,"parentId":%4,"parentNodeId":"%5","rootKey":"%6","synopsis":"","status":"","label":"","isGroup":false,"isRoot":%7,"hasChildren":%8,"wordCount":%9,"includeInCompile":true})")
      .arg(title)
      .arg(row)
      .arg(root ? 0 : 2)
      .arg(parent)
      .arg(parent < 0 ? QString() : QStringLiteral("node-%1").arg(parent))
      .arg(research ? QStringLiteral("research") : QStringLiteral("manuscript"))
      .arg(root ? QStringLiteral("true") : QStringLiteral("false"))
      .arg(root ? QStringLiteral("true") : QStringLiteral("false"))
      .arg(row + 1);
  }
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
  mutable int rowRequests = 0;

signals:
  void revisionChanged();
  void structureRevisionChanged();
  void contentRevisionChanged();
  void presentationRevisionChanged();
  void outlineModelDelta(const QString& kind, int first, int destination, int count);
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
    QCOMPARE(model.data(model.index(9'999), OutlineModel::ParentNodeIdRole).toString(),
             QStringLiteral("node-0"));
    QCOMPARE(model.data(model.index(9'999), OutlineModel::RootKeyRole).toString(),
             QStringLiteral("manuscript"));
    QVERIFY(model.data(model.index(0), OutlineModel::HasChildrenRole).toBool());
    QCOMPARE(model.data(model.index(4), OutlineModel::WordCountRole).toInt(), 5);
    QCOMPARE(source.rowRequests, 3); // rows 9999, 0, and 4, never once per role
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

  void typedDeltasPreserveRowStateAndInvalidateOnlyChangedCacheRows()
  {
    FakeRustOutline source;
    OutlineModel model;
    model.setSource(&source);
    QCOMPARE(model.data(model.index(4), OutlineModel::TitleRole).toString(), QStringLiteral("Node 4"));
    QCOMPARE(source.rowRequests, 1);

    emit source.outlineModelDelta(QStringLiteral("data"), 4, 0, 1);
    QCOMPARE(model.rowCount(), 10'000);
    QCOMPARE(model.data(model.index(4), OutlineModel::WordCountRole).toInt(), 5);
    QCOMPARE(source.rowRequests, 2);

    emit source.outlineModelDelta(QStringLiteral("insert"), 5, 0, 2);
    QCOMPARE(model.rowCount(), 10'002);
    emit source.outlineModelDelta(QStringLiteral("remove"), 5, 0, 2);
    QCOMPARE(model.rowCount(), 10'000);
  }

  void cardsKeepOnlyTheExpandableManuscriptHierarchy()
  {
    FakeRustOutline source;
    OutlineModel outline;
    outline.setSource(&source);
    CardsModel cards;
    cards.setSource(&outline);

    QCOMPARE(outline.data(outline.index(0), OutlineModel::RootKeyRole).toString(),
             QStringLiteral("manuscript"));
    QCOMPARE(outline.data(outline.index(2), OutlineModel::RootKeyRole).toString(),
             QStringLiteral("research"));
    QCOMPARE(cards.rowCount(), 9'997);
    QCOMPARE(cards.mapToSource(cards.index(0, 0)).row(), 1);
    QCOMPARE(cards.data(cards.index(0, 0), OutlineModel::RootKeyRole).toString(),
             QStringLiteral("manuscript"));
    QVariantMap collapsed;
    QVERIFY(cards.ancestorsExpanded(0, collapsed));
    collapsed.insert(QStringLiteral("node-0"), true);
    QVERIFY(!cards.ancestorsExpanded(0, collapsed));
  }
};

QTEST_MAIN(OutlineModelTest)
#include "tst_outline_model.moc"
