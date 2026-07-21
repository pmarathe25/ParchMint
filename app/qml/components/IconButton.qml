pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ToolButton {
    id: root
    required property string iconSource
    required property string accessibleName
    property string tooltip: accessibleName
    implicitWidth: DesignTokens.iconTouch
    implicitHeight: DesignTokens.iconTouch
    Accessible.name: accessibleName
    hoverEnabled: true
    ToolTip.visible: hovered
    ToolTip.text: tooltip
    contentItem: Image {
        source: root.iconSource
        sourceSize.width: DesignTokens.iconSize
        sourceSize.height: DesignTokens.iconSize
        width: DesignTokens.iconSize
        height: DesignTokens.iconSize
        anchors.centerIn: parent
        fillMode: Image.PreserveAspectFit
        opacity: root.enabled ? 1 : .45
    }
}
