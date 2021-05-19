using System;
using System.Collections.Generic;
using System.ComponentModel.DataAnnotations;
using System.Linq;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using HotChocolate;
using HotChocolate.Types.Relay;
using Microsoft.AspNetCore.Identity;
using RibbleChatServer.Data;

namespace RibbleChatServer.Models
{
    // we override a bunch of fields either to hide them or to enforce non-nullability
    [Node]
    public partial class User : IdentityUser<Guid>
    {
        public static async ValueTask<User> GetUserAsync(MainDbContext db, Guid id) =>
            await db.Users.FindAsync(id);

        public User(string UserName, string Email) =>
            (this.UserName, this.Email) = (UserName, Email);

        public override Guid Id { get; set; }
        public override string UserName { get; set; } = null!;
        public override string Email { get; set; } = null!;


        [JsonIgnore]
        [GraphQLIgnore]
        public override string PasswordHash { get; set; } = null!;

        [GraphQLIgnore]
        [JsonIgnore]
        public override bool EmailConfirmed { get; set; }

        [JsonIgnore]
        [GraphQLIgnore]
        public override bool PhoneNumberConfirmed { get; set; }

        [JsonIgnore]
        [GraphQLIgnore]
        public override string SecurityStamp { get; set; } = null!;

        [JsonIgnore]
        [GraphQLIgnore]
        public override string NormalizedUserName { get; set; } = null!;

        [JsonIgnore]
        [GraphQLIgnore]
        public override string NormalizedEmail { get; set; } = null!;

        public override string? PhoneNumber { get; set; }

        [JsonIgnore]
        [GraphQLIgnore]
        public override int AccessFailedCount { get; set; }

        public List<Group> Groups { get; set; } = null!;

        public static explicit operator UserResponse(User g) => new UserResponse(
            Id: g.Id,
            UserName: g.UserName,
            Email: g.Email,
            Groups: g.Groups.Select(g => (GroupResponse)g)
        );
    }

    public record UserResponse(
        [property: JsonPropertyName("id")] Guid Id,
        [property: JsonPropertyName("username")] string UserName,
        [property: JsonPropertyName("email")] string Email,
        [property: JsonPropertyName("groups")] IEnumerable<GroupResponse> Groups
    );

    public record RegisterUserInfo(
        [Required] string Email,
        [Required] string Username,
        [Required] string Password
    );

    public record LoginUserInfo
    {
        [Required]
        public string UsernameOrEmail { get; set; } = null!;

        [Required]
        public string Password { get; set; } = null!;
    }
}

