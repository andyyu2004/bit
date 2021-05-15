using Cassandra;
using System.Threading.Tasks;
using Microsoft.Extensions.Logging;
using RibbleChatServer.Models;

namespace RibbleChatServer.Data
{
    public class MessageDb : IMessageDb
    {
        private ISession? session;

        private Cassandra.Data.Linq.Table<ChatMessage>? messageTable;

        private readonly ILogger<MessageDb> logger;

        const string KEYSPACE = "ribble";

        public MessageDb(ILogger<MessageDb> logger)
        {
            this.logger = logger;
            Task.Run(Init);
        }

        public async Task Init()
        {
            try
            {
                var cluster = Cluster.Builder()
                    .AddContactPoint("ribble-scylla")
                    .WithPort(9042)
                    .Build();
                session = await cluster.ConnectAsync();
                try
                {
                    session.ChangeKeyspace(KEYSPACE);
                }
                catch (InvalidQueryException)
                {
                    session.CreateKeyspaceIfNotExists(KEYSPACE);
                    session.ChangeKeyspace(KEYSPACE);
                }
                // await session.UserDefinedTypes.DefineAsync(UdtMap.For<Message>());

                var table = new Cassandra.Data.Linq.Table<ChatMessage>(session);
                await table.CreateIfNotExistsAsync();
                this.messageTable = table;
            }
            catch (NoHostAvailableException e)
            {
                logger.LogError(e.Message);
                await Task.Delay(5000);
                await Init();
            }
        }

        private async Task<Cassandra.Data.Linq.Table<ChatMessage>> getMessageTable()
        {
            if (messageTable is null) await Init();
            return messageTable!;
        }

        public async Task AddMessage(ChatMessage msg)
        {
            var messages = await getMessageTable();
            await messages.Insert(msg).ExecuteAsync();
        }

        ~MessageDb() => session?.Dispose();
    }
}

