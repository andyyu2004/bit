using System;
using Cassandra;
using Microsoft.AspNetCore.SignalR;
using RibbleChatServer.Data;
using RibbleChatServer.Models;
using System.Threading.Tasks;

namespace RibbleChatServer.Services
{
    public class ChatHub : Hub
    {
        private IMessageDb chatDb;

        public ChatHub(IMessageDb chatDb)
        {
            this.chatDb = chatDb;
        }

        public async Task JoinGroups(string[] groupIds)
        {
            foreach (var groupId in groupIds)
            {
                await Groups.AddToGroupAsync(Context.ConnectionId, groupId);
                await Clients.Group(groupId).SendAsync("joined-group", groupId, Context.ConnectionId);
            }
        }

        public async Task SendMessage(SendMessageRequest request)
        {
            var (authorId, authorName, groupId, content) = request;
            var message = new ChatMessage(
                MessageId: Guid.NewGuid(),
                Timestamp: DateTimeOffset.UtcNow,
                GroupId: groupId,
                AuthorId: authorId,
                AuthorUsername: authorName,
                Content: content
            );
            await Clients.Group(groupId.ToString())
                .SendAsync("message-received", message);
            await chatDb.AddMessage(message);
        }
    }

}
