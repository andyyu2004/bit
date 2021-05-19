using System;
using System.Diagnostics.CodeAnalysis;
using Microsoft.AspNetCore.Identity;
using Microsoft.AspNetCore.Identity.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore;
using RibbleChatServer.Models;

namespace RibbleChatServer.Data
{
    public class MainDbContext : IdentityDbContext<User, Role, Guid>
    {
        public DbSet<Group> Groups { get; set; } = null!;

        public MainDbContext([NotNullAttribute] DbContextOptions options) : base(options)
        {
        }

        protected override void OnModelCreating(ModelBuilder builder)
        {
            base.OnModelCreating(builder);
            builder.HasPostgresExtension("uuid-ossp");
            builder.Entity<User>(entity => entity.ToTable("users"));
            builder.Entity<Role>(entity => entity.ToTable("roles"));
            builder.Entity<IdentityUserRole<Guid>>(entity => entity.ToTable("user_roles"));
            builder.Entity<IdentityUserClaim<Guid>>(entity => entity.ToTable("user_claims"));
            builder.Entity<IdentityUserLogin<Guid>>(entity => entity.ToTable("user_logins"));
            builder.Entity<IdentityUserToken<Guid>>(entity => entity.ToTable("user_tokens"));
            builder.Entity<IdentityRoleClaim<Guid>>(entity => entity.ToTable("role_claims"));
        }
    }
}
