using Microsoft.EntityFrameworkCore.Migrations;

namespace RibbleChatServer.Migrations
{
    public partial class RenameGroupTable : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropForeignKey(
                name: "fk_group_user_group_groups_id",
                table: "group_user");

            migrationBuilder.DropPrimaryKey(
                name: "pk_group",
                table: "group");

            migrationBuilder.RenameTable(
                name: "group",
                newName: "groups");

            migrationBuilder.AddPrimaryKey(
                name: "pk_groups",
                table: "groups",
                column: "id");

            migrationBuilder.AddForeignKey(
                name: "fk_group_user_groups_groups_id",
                table: "group_user",
                column: "groups_id",
                principalTable: "groups",
                principalColumn: "id",
                onDelete: ReferentialAction.Cascade);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropForeignKey(
                name: "fk_group_user_groups_groups_id",
                table: "group_user");

            migrationBuilder.DropPrimaryKey(
                name: "pk_groups",
                table: "groups");

            migrationBuilder.RenameTable(
                name: "groups",
                newName: "group");

            migrationBuilder.AddPrimaryKey(
                name: "pk_group",
                table: "group",
                column: "id");

            migrationBuilder.AddForeignKey(
                name: "fk_group_user_group_groups_id",
                table: "group_user",
                column: "groups_id",
                principalTable: "group",
                principalColumn: "id",
                onDelete: ReferentialAction.Cascade);
        }
    }
}
