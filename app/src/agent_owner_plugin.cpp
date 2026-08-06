#include "agent_owner_plugin.h"

#include <QTimer>

#include "logos_api.h"
#include "logos_api_client.h"

AgentOwnerPlugin::AgentOwnerPlugin(QObject* parent)
    : AgentOwnerSimpleSource(parent)
{
}

AgentOwnerPlugin::~AgentOwnerPlugin()
{
    delete m_client;
}

void AgentOwnerPlugin::initLogos(LogosAPI* api)
{
    m_api = api;
    setBackend(this);
    ensureClient();
    QTimer::singleShot(0, this, [this]() { refresh(); });
}

void AgentOwnerPlugin::ensureClient()
{
    if (m_client || !m_api) {
        return;
    }
    m_client = new LogosAPIClient(
        QStringLiteral("agent"),
        QStringLiteral("agent_owner"),
        m_api->getTokenManager(),
        this);
}

QString AgentOwnerPlugin::invokeAgent(const QString& method, const QVariantList& args)
{
    ensureClient();
    if (!m_client) {
        return QStringLiteral("{\"ok\":false,\"error\":\"no agent client\"}");
    }
    switch (args.size()) {
        case 0:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method).toString();
        case 1:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method, args[0]).toString();
        case 2:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method, args[0], args[1]).toString();
        default:
            return QStringLiteral("{\"ok\":false,\"error\":\"unsupported argument count\"}");
    }
}

void AgentOwnerPlugin::refresh()
{
    setAgentVersion(invokeAgent(QStringLiteral("agentVersionJson")));
    setSkillsJson(invokeAgent(QStringLiteral("skillsJson")));
}

QString AgentOwnerPlugin::invokeSkill(QString name, QString argsJson)
{
    return invokeAgent(QStringLiteral("invokeSkillJson"), QVariantList{ name, argsJson });
}
