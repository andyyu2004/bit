using System;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.AspNetCore.Mvc;
using RibbleChatServer.Data;
using RibbleChatServer.Models;

namespace RibbleChatServer.Controllers
{

    [ApiController]
    public class ChatController : ControllerBase
    {
        private readonly MainDbContext userDb;

        public ChatController(MainDbContext userDb) => this.userDb = userDb;

        [HttpPost]
        [Route("/api/chat/groups")]
        public async Task<ActionResult<GroupResponse>> CreateGroup([FromBody] CreateGroupRequest request)
        {
            var (groupName, userIds) = request;
            var newGroup = new Group(name: groupName);
            var entity = await userDb.AddAsync(newGroup);
            var group = entity.Entity;
            group.Users.AddRange(userIds.Select(userId => userDb.Users.Find(userId)));
            await userDb.SaveChangesAsync();
            return (GroupResponse)group;
        }

        [HttpGet]
        [Route("/api/chat/groups/{userId}")]
        public async Task<ActionResult<GroupResponse[]>> GroupsForUser(Guid userId)
        {
            var user = await userDb.Users.FindAsync(userId);
            if (user is null) return NotFound();
            return Ok(user.Groups);
        }
    }
}