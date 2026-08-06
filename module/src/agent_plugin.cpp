#include "agent_plugin.h"

AgentPlugin::AgentPlugin() = default;
AgentPlugin::~AgentPlugin() = default;

void AgentPlugin::initLogos(LogosAPI* api)
{
    m_api = api;
    // Also set the base-class handle the Logos Core runtime uses to route calls.
    logosAPI = api;
}

QString AgentPlugin::health()
{
    const QString version = m_ffi.version();
    if (version.contains(QStringLiteral("\"ok\":true"))) {
        return QStringLiteral("{\"ok\":true}");
    }
    return QStringLiteral("{\"ok\":false,\"error\":\"agent core not loaded\"}");
}

QString AgentPlugin::agentVersionJson()
{
    return m_ffi.version();
}

QString AgentPlugin::skillsJson()
{
    return m_ffi.skills();
}

QString AgentPlugin::startSessionJson(const QString& accountId)
{
    if (m_ffi.startSession(accountId)) {
        return QStringLiteral("{\"ok\":true}");
    }
    return QStringLiteral("{\"ok\":false,\"error\":\"could not start session\"}");
}

QString AgentPlugin::invokeSkillJson(const QString& name, const QString& argsJson)
{
    return m_ffi.invoke(name, argsJson);
}
