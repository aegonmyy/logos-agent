#include "agent_plugin.h"

AgentPlugin::AgentPlugin() = default;
AgentPlugin::~AgentPlugin() = default;

void AgentPlugin::initLogos(LogosAPI* api)
{
    m_api = api;
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
